//! API-payer gas funder.
//!
//! Keeps the lit-api-server payer pool topped up so `new_account` (and other
//! on-chain writes) never fail with `insufficient funds for gas`.
//!
//! Background: lit-api-server signs writes from a pool of payer wallets whose
//! keys live in the dstack TEE. Pool signer-selection is round-robin and *not*
//! balance-aware, so a single drained wallet causes intermittent 500s. The
//! in-TEE admin payer only rebalances on pool *resize*, never continuously.
//! lit-payments runs out of the TEE hot path (Railway, single instance), so it
//! carries a small hot wallet and tops payers up on a schedule.
//!
//! ## Modes
//! - OBSERVE (`GAS_FUNDER_ENABLED` unset/false): read balances, email
//!   alerts, broadcast nothing. Safe first deploy.
//! - ACTIVE (`GAS_FUNDER_ENABLED=true`): top up any payer below low-water up
//!   to high-water, subject to a per-tx cap and a rolling 24h cap.
//!
//! ## Safety rails
//! - Funds the *live* `get_api_payers` set each tick (the pool rotates).
//! - Per-tx ceiling + rolling-24h ceiling (DB-summed); a bug/compromise can't
//!   drain the hot wallet in one tick.
//! - Records a `pending` row BEFORE each send; single-instance + await-receipt
//!   makes nonces sequential and avoids double-spend on restart.
//! - "Reload me" email when the hot wallet itself runs low (the one wallet a
//!   human watches); routine successful top-ups are silent (info logs only).

mod db;

use std::time::Duration;

use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy_primitives::{Address, TxKind, U256};
use anyhow::{Context, Result};
use sqlx::PgPool;
use tokio::time as tokio_time;

use crate::chain::format_address;
use crate::config::{Config, GasFunderConfig};
use crate::mail::Mailer;

/// Cooldown for the "hot wallet low — reload me" email.
const HOTWALLET_ALERT_COOLDOWN_SECS: i64 = 6 * 3600;
/// Cooldown for failure / cap-reached / can't-cover alerts.
const FAILURE_ALERT_COOLDOWN_SECS: i64 = 3600;
/// Cooldown for the OBSERVE-mode "would have funded" email.
const OBSERVE_ALERT_COOLDOWN_SECS: i64 = 6 * 3600;
/// A `pending` funding row older than this is flagged as a possible
/// interrupted send (never auto-retried — that could double-spend).
const STALE_PENDING_SECS: i64 = 600;
/// Gas units assumed for a plain value transfer when sizing the hot-wallet
/// coverage reserve.
const VALUE_TRANSFER_GAS: u64 = 21_000;

/// Spawn the background funder loop. No-op (logs once) when the funder is not
/// configured. Returns immediately; the loop runs for the process lifetime.
pub fn spawn(config: Config, pool: PgPool, mailer: Mailer) {
    let Some(funder) = config.gas_funder.clone() else {
        tracing::info!("gas_funder: not configured (GAS_FUNDER_PRIVATE_KEY unset); skipping");
        return;
    };
    let interval_secs = funder.interval_secs.max(30);
    tracing::info!(
        mode = if funder.enabled { "ACTIVE" } else { "OBSERVE" },
        chain_id = funder.chain_id,
        interval_secs,
        hot_wallet = %format_address(funder.signer.address()),
        include_admin = funder.include_admin,
        "gas_funder: starting ({})",
        if funder.enabled {
            "will broadcast funding txns"
        } else {
            "alerts only, no sends"
        }
    );
    tokio::spawn(async move {
        let mut ticker = tokio_time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&config, &funder, &pool, &mailer).await {
                tracing::warn!("gas_funder tick failed: {e:?}");
            }
        }
    });
}

/// One funder pass: read balances, decide top-ups, alert, and (in ACTIVE
/// mode) fund. Errors from individual sends are caught and alerted; only
/// setup failures (RPC/payer-list unreachable) bubble up to the caller.
async fn run_once(
    config: &Config,
    funder: &GasFunderConfig,
    pool: &PgPool,
    mailer: &Mailer,
) -> Result<()> {
    if let Ok(n) = db::count_stale_pending(pool, STALE_PENDING_SECS).await
        && n > 0
    {
        tracing::warn!(
            "gas_funder: {n} pending funding event(s) older than {STALE_PENDING_SECS}s — \
             possible interrupted send; not auto-retried (inspect gas_funding_events)"
        );
    }

    let provider = build_provider(funder)?;

    let hot_addr = funder.signer.address();
    let hot_balance = provider
        .get_balance(hot_addr)
        .await
        .context("reading hot wallet balance")?;

    let payers = fetch_payers(config, funder)
        .await
        .context("fetching API payer set")?;
    if payers.is_empty() {
        tracing::warn!("gas_funder: get_api_payers returned no addresses; nothing to do");
        return Ok(());
    }

    let mut balances: Vec<(Address, U256)> = Vec::with_capacity(payers.len());
    for addr in &payers {
        let bal = provider
            .get_balance(*addr)
            .await
            .with_context(|| format!("reading balance of {}", format_address(*addr)))?;
        balances.push((*addr, bal));
    }

    let funded_24h = db::funded_last_24h(pool, funder.chain_id as i64)
        .await
        .unwrap_or(U256::ZERO);
    let remaining_budget = funder.daily_cap_wei.saturating_sub(funded_24h);

    let low_count = balances
        .iter()
        .filter(|(_, b)| *b < funder.low_water_wei)
        .count();

    tracing::info!(
        mode = if funder.enabled { "ACTIVE" } else { "OBSERVE" },
        hot_wallet = %format_address(hot_addr),
        hot_balance_wei = %hot_balance,
        payers = payers.len(),
        below_low_water = low_count,
        funded_24h_wei = %funded_24h,
        remaining_budget_wei = %remaining_budget,
        "gas_funder: tick"
    );

    // "Reload me" — the one wallet a human watches. Independent of whether any
    // payer is low right now.
    if hot_balance < funder.hotwallet_min_wei {
        let body = format!(
            "The gas funder hot wallet is running low.\n\n\
             Hot wallet : {}\n\
             Balance    : {} wei\n\
             Reload at  : {} wei (GAS_FUNDER_HOTWALLET_MIN_WEI)\n\n\
             Reload it so it can keep topping up the lit-api-server API payer wallets.",
            format_address(hot_addr),
            hot_balance,
            funder.hotwallet_min_wei,
        );
        maybe_alert(
            pool,
            mailer,
            funder,
            "hotwallet_low",
            HOTWALLET_ALERT_COOLDOWN_SECS,
            "[lit-payments] Gas funder hot wallet low — reload needed",
            &body,
        )
        .await;
    }

    if low_count == 0 {
        return Ok(());
    }

    // Payers are low but the rolling cap is spent — alert and stop. (Checked
    // before planning because decide_topups would otherwise return an empty
    // plan and we'd say nothing.)
    if remaining_budget.is_zero() {
        let body = format!(
            "The gas funder hit its rolling 24h cap ({} wei) but {} payer wallet(s) are still \
             below low-water ({} wei):\n\n{}\n\nRaise GAS_FUNDER_DAILY_CAP_WEI or investigate \
             unusually fast gas burn.",
            funder.daily_cap_wei,
            low_count,
            funder.low_water_wei,
            format_low_list(&balances, funder.low_water_wei),
        );
        maybe_alert(
            pool,
            mailer,
            funder,
            "daily_cap_reached",
            FAILURE_ALERT_COOLDOWN_SECS,
            "[lit-payments] Gas funder daily cap reached — payers still low",
            &body,
        )
        .await;
        return Ok(());
    }

    let plan = decide_topups(
        &balances,
        funder.low_water_wei,
        funder.high_water_wei,
        funder.max_tx_wei,
        remaining_budget,
    );
    if plan.is_empty() {
        return Ok(());
    }

    // OBSERVE mode: report what we WOULD do, broadcast nothing.
    if !funder.enabled {
        let body = format!(
            "gas_funder is in OBSERVE mode (GAS_FUNDER_ENABLED is not true), so the following \
             top-ups were NOT sent:\n\n{}\n\nSet GAS_FUNDER_ENABLED=true to enable automatic \
             funding.",
            format_plan(&plan),
        );
        maybe_alert(
            pool,
            mailer,
            funder,
            "observe_would_fund",
            OBSERVE_ALERT_COOLDOWN_SECS,
            "[lit-payments] API payer wallets low (gas funder in observe mode)",
            &body,
        )
        .await;
        tracing::info!(
            planned = plan.len(),
            "gas_funder: OBSERVE mode — top-ups planned but not sent"
        );
        return Ok(());
    }

    // ACTIVE: make sure the hot wallet can physically cover the plan + gas
    // before we start broadcasting, so we don't get a half-funded round.
    let total_planned = plan
        .iter()
        .fold(U256::ZERO, |acc, p| acc.saturating_add(p.amount_wei));
    let gas_price = U256::from(provider.get_gas_price().await.unwrap_or_default());
    // 2x headroom over the nominal value-transfer gas, per planned send.
    let gas_reserve = gas_price
        .saturating_mul(U256::from(VALUE_TRANSFER_GAS))
        .saturating_mul(U256::from(plan.len() as u64))
        .saturating_mul(U256::from(2u64));
    if hot_balance < total_planned.saturating_add(gas_reserve) {
        let body = format!(
            "The gas funder hot wallet cannot cover this round's planned top-ups.\n\n\
             Hot wallet     : {}\n\
             Balance        : {} wei\n\
             Planned total  : {} wei\n\
             Gas reserve    : ~{} wei\n\n\
             Reload the hot wallet. No funding txns were sent this round.",
            format_address(hot_addr),
            hot_balance,
            total_planned,
            gas_reserve,
        );
        maybe_alert(
            pool,
            mailer,
            funder,
            "hotwallet_insufficient",
            FAILURE_ALERT_COOLDOWN_SECS,
            "[lit-payments] Gas funder hot wallet cannot cover planned top-ups",
            &body,
        )
        .await;
        return Ok(());
    }

    let mut failures: Vec<String> = Vec::new();
    for planned in &plan {
        match execute_topup(&provider, funder, pool, planned).await {
            Ok(tx_hash) => tracing::info!(
                recipient = %format_address(planned.recipient),
                amount_wei = %planned.amount_wei,
                tx_hash = %tx_hash,
                "gas_funder: funded payer"
            ),
            Err(e) => {
                tracing::error!(
                    recipient = %format_address(planned.recipient),
                    amount_wei = %planned.amount_wei,
                    "gas_funder: funding failed: {e:?}"
                );
                failures.push(format!(
                    "  {} (+{} wei): {e}",
                    format_address(planned.recipient),
                    planned.amount_wei
                ));
            }
        }
    }

    if !failures.is_empty() {
        let body = format!(
            "gas_funder failed to fund {} payer wallet(s) this round:\n\n{}",
            failures.len(),
            failures.join("\n"),
        );
        maybe_alert(
            pool,
            mailer,
            funder,
            "fund_failed",
            FAILURE_ALERT_COOLDOWN_SECS,
            "[lit-payments] Gas funder failed to fund payer(s)",
            &body,
        )
        .await;
    }

    Ok(())
}

/// A single planned top-up: send `amount_wei` to `recipient` (whose current
/// balance is `balance_wei`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTopup {
    pub recipient: Address,
    pub amount_wei: U256,
    pub balance_wei: U256,
}

/// Decide which payers to top up and by how much. Pure: no chain or DB access,
/// so it's exhaustively unit-testable.
///
/// For each payer below `low_water`, plan to bring it up to `high_water`,
/// clamped by the per-tx ceiling `max_tx` and the `remaining_budget` (the
/// rolling-24h cap minus what's already been spent). Budget is consumed
/// greedily in iteration order; once exhausted, remaining low payers are left
/// for a future tick.
pub fn decide_topups(
    balances: &[(Address, U256)],
    low_water: U256,
    high_water: U256,
    max_tx: U256,
    mut remaining_budget: U256,
) -> Vec<PlannedTopup> {
    let mut plan = Vec::new();
    for &(recipient, balance) in balances {
        if balance >= low_water {
            continue;
        }
        let mut amount = high_water.saturating_sub(balance);
        if amount > max_tx {
            amount = max_tx;
        }
        if amount > remaining_budget {
            amount = remaining_budget;
        }
        if amount.is_zero() {
            continue;
        }
        plan.push(PlannedTopup {
            recipient,
            amount_wei: amount,
            balance_wei: balance,
        });
        remaining_budget = remaining_budget.saturating_sub(amount);
        if remaining_budget.is_zero() {
            break;
        }
    }
    plan
}

/// Record a `pending` row, broadcast the transfer, await the receipt, then
/// transition the row to `sent`/`failed`. Returns the tx hash on success.
async fn execute_topup(
    provider: &DynProvider,
    funder: &GasFunderConfig,
    pool: &PgPool,
    planned: &PlannedTopup,
) -> Result<String> {
    let recipient = format_address(planned.recipient);
    let id = db::insert_pending(
        pool,
        funder.chain_id as i64,
        &recipient,
        &planned.amount_wei.to_string(),
        &planned.balance_wei.to_string(),
    )
    .await
    .context("recording pending funding event")?;

    let req = TransactionRequest {
        to: Some(TxKind::Call(planned.recipient)),
        value: Some(planned.amount_wei),
        chain_id: Some(funder.chain_id),
        ..Default::default()
    };

    match provider.send_transaction(req).await {
        Ok(pending) => match pending.get_receipt().await {
            Ok(receipt) => {
                let tx_hash = format!("{:#x}", receipt.transaction_hash);
                if receipt.status() {
                    db::mark_sent(pool, id, &tx_hash).await?;
                    Ok(tx_hash)
                } else {
                    let msg = format!("transaction reverted on-chain: {tx_hash}");
                    db::mark_failed(pool, id, &msg).await?;
                    anyhow::bail!("{msg}")
                }
            }
            Err(e) => {
                db::mark_failed(pool, id, &format!("receipt error: {e}")).await?;
                Err(anyhow::Error::from(e).context("awaiting funding tx receipt"))
            }
        },
        Err(e) => {
            db::mark_failed(pool, id, &format!("send error: {e}")).await?;
            Err(anyhow::Error::from(e).context("broadcasting funding tx"))
        }
    }
}

/// Build a wallet-backed provider for the funder's chain. The default fillers
/// (nonce, gas, fee, chain-id) handle everything a plain value transfer needs.
fn build_provider(funder: &GasFunderConfig) -> Result<DynProvider> {
    let url: reqwest::Url = funder
        .rpc_url
        .parse()
        .context("GAS_FUNDER_RPC_URL is not a valid URL")?;
    Ok(ProviderBuilder::new()
        .wallet(funder.signer.clone())
        .connect_http(url)
        .erased())
}

/// Fetch the live API payer set from lit-api-server (and the admin payer when
/// `include_admin`). These are public, unauthenticated read endpoints.
async fn fetch_payers(config: &Config, funder: &GasFunderConfig) -> Result<Vec<Address>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("building payer-list HTTP client")?;

    let base = &config.lit_api_server_base_url;
    let payers_url = format!("{base}/core/v1/get_api_payers");
    let payer_strings: Vec<String> = client
        .get(&payers_url)
        .send()
        .await
        .context("GET get_api_payers")?
        .error_for_status()
        .context("get_api_payers returned non-success")?
        .json()
        .await
        .context("decoding get_api_payers response")?;

    let mut out = Vec::with_capacity(payer_strings.len() + 1);
    for p in payer_strings {
        let addr = p
            .trim()
            .parse::<Address>()
            .with_context(|| format!("get_api_payers returned a non-address entry: {p:?}"))?;
        if !out.contains(&addr) {
            out.push(addr);
        }
    }

    if funder.include_admin {
        match fetch_admin_payer(&client, base).await {
            Ok(Some(addr)) if !out.contains(&addr) => out.push(addr),
            Ok(_) => {}
            Err(e) => tracing::warn!("gas_funder: could not fetch admin payer (skipping): {e}"),
        }
    }

    Ok(out)
}

async fn fetch_admin_payer(client: &reqwest::Client, base: &str) -> Result<Option<Address>> {
    let url = format!("{base}/core/v1/get_admin_api_payer");
    let raw: String = client
        .get(&url)
        .send()
        .await
        .context("GET get_admin_api_payer")?
        .error_for_status()
        .context("get_admin_api_payer returned non-success")?
        .json()
        .await
        .context("decoding get_admin_api_payer response")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.parse::<Address>().with_context(|| {
        format!("get_admin_api_payer returned a non-address: {raw:?}")
    })?))
}

/// Send an alert email if its cooldown has elapsed. Failures to send are
/// logged, never propagated — alerting must not break the funder loop.
async fn maybe_alert(
    pool: &PgPool,
    mailer: &Mailer,
    funder: &GasFunderConfig,
    key: &str,
    cooldown_secs: i64,
    subject: &str,
    body: &str,
) {
    match db::should_alert(pool, key, cooldown_secs).await {
        Ok(true) => {
            let html = format!("<pre>{}</pre>", html_escape(body));
            if let Err(e) = mailer.send(&funder.alert_email, subject, &html, body).await {
                tracing::warn!("gas_funder: alert email '{key}' failed to send: {e}");
            }
        }
        Ok(false) => {
            tracing::info!("gas_funder: alert '{key}' suppressed (within cooldown)");
        }
        Err(e) => tracing::warn!("gas_funder: alert cooldown check for '{key}' failed: {e}"),
    }
}

fn format_plan(plan: &[PlannedTopup]) -> String {
    plan.iter()
        .map(|p| {
            format!(
                "  {} : balance {} wei  ->  send +{} wei",
                format_address(p.recipient),
                p.balance_wei,
                p.amount_wei
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_low_list(balances: &[(Address, U256)], low_water: U256) -> String {
    balances
        .iter()
        .filter(|(_, b)| *b < low_water)
        .map(|(a, b)| format!("  {} : {} wei", format_address(*a), b))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Minimal HTML escaping for the alert body (we wrap it in `<pre>`).
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn addr(n: u8) -> Address {
        let mut bytes = [0u8; 20];
        bytes[19] = n;
        Address::from(bytes)
    }

    fn wei(s: &str) -> U256 {
        U256::from_str(s).unwrap()
    }

    #[test]
    fn skips_payers_at_or_above_low_water() {
        let balances = vec![
            (addr(1), wei("100")), // at low-water
            (addr(2), wei("200")), // above
        ];
        let plan = decide_topups(&balances, wei("100"), wei("500"), wei("1000"), wei("100000"));
        assert!(plan.is_empty());
    }

    #[test]
    fn tops_low_payer_up_to_high_water() {
        let balances = vec![(addr(1), wei("30"))];
        let plan = decide_topups(&balances, wei("100"), wei("500"), wei("1000"), wei("100000"));
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].recipient, addr(1));
        assert_eq!(plan[0].amount_wei, wei("470")); // 500 - 30
    }

    #[test]
    fn clamps_topup_to_max_tx() {
        let balances = vec![(addr(1), wei("0"))];
        let plan = decide_topups(&balances, wei("100"), wei("500"), wei("250"), wei("100000"));
        assert_eq!(plan[0].amount_wei, wei("250")); // capped by max_tx
    }

    #[test]
    fn clamps_to_remaining_budget_and_stops_when_exhausted() {
        let balances = vec![
            (addr(1), wei("0")),
            (addr(2), wei("0")),
            (addr(3), wei("0")),
        ];
        // Each wants 500, max_tx 500, but only 700 budget remains.
        let plan = decide_topups(&balances, wei("100"), wei("500"), wei("500"), wei("700"));
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].amount_wei, wei("500"));
        assert_eq!(plan[1].amount_wei, wei("200")); // remainder of the budget
    }

    #[test]
    fn zero_remaining_budget_plans_nothing() {
        let balances = vec![(addr(1), wei("0"))];
        let plan = decide_topups(&balances, wei("100"), wei("500"), wei("500"), U256::ZERO);
        assert!(plan.is_empty());
    }

    #[test]
    fn html_escape_neutralizes_markup() {
        assert_eq!(html_escape("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }
}
