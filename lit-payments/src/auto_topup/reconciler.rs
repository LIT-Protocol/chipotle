//! Auto top-up reconciler.
//!
//! Plan §6 / §9: the sync-credit webhook handler can fail mid-flow:
//!   - HTTP timeout on `paymentIntents.create` — the PI may still
//!     succeed at Stripe but we never see the response.
//!   - DB write timeout between `INSERT auto_topup_credits` and
//!     `balance_transactions` — credit row exists, balance not credited.
//!   - Process crash anywhere in the middle.
//!
//! The reconciler runs two passes every `RECONCILER_INTERVAL_SECS`:
//!
//! **Pass A — DB-driven (glitch's PR review #3)**: query
//! `auto_topup_credits` for rows where `stripe_balance_transaction_id IS
//! NULL` and replay the balance_transactions write for each. The DB row
//! is the source of truth — we already have `(pi_id, customer_id,
//! amount)`, so Stripe only needs to be hit by PI id, which has no time
//! window. Pre-fix this lived behind a 7-day `payment_intents.list`
//! scan; partials older than that were stranded forever.
//!
//! **Pass B — Stripe-list-driven**: for each enabled customer, scan
//! `payment_intents.list` over the last 7 days for succeeded auto_topup
//! PIs that have no `auto_topup_credits` row at all. Catches the rarer
//! "succeeded at Stripe but service crashed before INSERT" path.
//!
//! Spawned from main.rs at startup; runs for the lifetime of the
//! process. The process exits non-cleanly on shutdown (no graceful
//! drain) — that's fine because the next reconciler tick on the
//! restarted process picks up any work that was interrupted.

use std::collections::BTreeSet;
use std::time::Duration;

use ::time::OffsetDateTime;
use anyhow::Context;
use lit_billing_core::StripeClient;
use serde_json::Value;
use sqlx::PgPool;
use tokio::time as tokio_time;

use crate::auto_topup::db;
use crate::config::Config;
use crate::internal::client as internal_client;

const LOOKBACK_DAYS: i64 = 7;

/// Minimum age before the reconciler retries a partial credit row. The
/// webhook handler typically writes the balance_transactions row within
/// a few hundred ms of inserting the credit row; a fresh partial row is
/// almost certainly the live webhook flow still in-flight, not a real
/// orphan. Retrying immediately would race with the webhook on the
/// balance_transactions write — safe (Stripe Idempotency-Key dedupes)
/// but wasteful and noisy in logs. 60s is a generous floor that still
/// lets a true crash mid-flow recover within one reconciler tick.
const MIN_PARTIAL_AGE_SECS: i64 = 60;

/// Spawn the reconciler. Returns immediately; the loop runs in the
/// background until the process exits.
pub fn spawn(config: Config, stripe: StripeClient, pool: PgPool) {
    let interval_secs = config.reconciler_interval_secs.max(10) as u64;
    tracing::info!(
        "auto_topup reconciler: interval = {interval_secs}s, lookback = {LOOKBACK_DAYS}d"
    );
    tokio::spawn(async move {
        let mut ticker = tokio_time::interval(Duration::from_secs(interval_secs));
        // First tick fires immediately; that's the desired behaviour
        // (run-on-startup so a process restart catches in-flight work).
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&config, &stripe, &pool).await {
                tracing::warn!("auto_topup reconciler tick failed: {e:?}");
            }
        }
    });
}

/// Two-pass sweep. Public for tests; the production scheduler in
/// [`spawn`] is the only other caller.
pub async fn run_once(config: &Config, stripe: &StripeClient, pool: &PgPool) -> anyhow::Result<()> {
    // Pass A: DB-driven repair of partial credit rows.
    if let Err(e) = repair_partial_credits(config, stripe, pool).await {
        tracing::warn!("reconciler pass A (partial repair) failed: {e:?}");
    }

    // Pass B: Stripe-list-driven sweep for orphan PIs (no DB row at all).
    let enabled = db::list_enabled_customers(pool)
        .await
        .context("list enabled customers")?;
    let mut all: BTreeSet<String> = BTreeSet::new();
    all.extend(enabled);
    if all.is_empty() {
        return Ok(());
    }
    let since_unix =
        (OffsetDateTime::now_utc() - ::time::Duration::days(LOOKBACK_DAYS)).unix_timestamp();
    for customer_id in all {
        if let Err(e) = reconcile_customer(config, stripe, pool, &customer_id, since_unix).await {
            tracing::warn!(customer_id, "reconcile_customer failed: {e:?}");
        }
    }
    Ok(())
}

/// Glitch's PR review #3: DB-driven partial-credit repair. Walks every
/// `auto_topup_credits` row with `stripe_balance_transaction_id IS NULL`
/// — these are PIs we know succeeded (we wouldn't have inserted the
/// row otherwise) where the balance_transactions write didn't land.
/// We fetch each PI by id directly (no time window), verify it's still
/// `succeeded` at Stripe, then re-issue the balance_tx with the stable
/// `credit:{pi_id}` idempotency key.
async fn repair_partial_credits(
    config: &Config,
    stripe: &StripeClient,
    pool: &PgPool,
) -> anyhow::Result<()> {
    let partials = db::list_partial_credit_rows(pool)
        .await
        .context("list partial credit rows")?;
    if partials.is_empty() {
        return Ok(());
    }
    let now = OffsetDateTime::now_utc();
    for partial in partials {
        // Age-gate: a row younger than MIN_PARTIAL_AGE_SECS is almost
        // certainly the live webhook's in-flight balance_tx write.
        // Repairing it would race; the idempotency-key makes the race
        // safe but wasteful.
        let age = now - partial.credited_at;
        if age.whole_seconds() < MIN_PARTIAL_AGE_SECS {
            continue;
        }
        let pi_id = partial.payment_intent_id;
        let customer_id = partial.customer_id;
        let amount = partial.amount_cents;
        // Verify the PI is still succeeded at Stripe before posting a
        // balance_tx for it. Defence against an attacker who somehow
        // inserted a row, and against PI reversal edge cases.
        let pi = match stripe.get(&format!("payment_intents/{pi_id}"), &[]).await {
            Ok(r) => r.body,
            Err(e) => {
                tracing::warn!(
                    customer_id,
                    pi_id,
                    "reconciler: stripe GET {pi_id} failed; will retry next tick: {e}"
                );
                continue;
            }
        };
        if pi.get("status").and_then(|v| v.as_str()) != Some("succeeded") {
            tracing::warn!(
                customer_id,
                pi_id,
                status = pi.get("status").and_then(|v| v.as_str()).unwrap_or(""),
                "reconciler: skipping partial whose PI is not succeeded"
            );
            continue;
        }
        if pi.pointer("/metadata/source").and_then(|v| v.as_str()) != Some("auto_topup") {
            tracing::warn!(
                customer_id,
                pi_id,
                "reconciler: skipping partial whose PI is not metadata.source=auto_topup"
            );
            continue;
        }
        tracing::warn!(
            customer_id,
            pi_id,
            "reconciler: completing partial credit (DB-driven)"
        );
        let bt_id = match write_balance_transaction(stripe, &customer_id, &pi_id, amount).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(
                    customer_id,
                    pi_id,
                    "reconciler: balance_tx write failed: {e}"
                );
                continue;
            }
        };
        if let Err(e) = db::mark_credit_completed(pool, &pi_id, &bt_id).await {
            tracing::warn!(
                customer_id,
                pi_id,
                "reconciler: mark_credit_completed failed: {e}"
            );
            continue;
        }
        let _ = invalidate_cache(config, &customer_id).await;
    }
    Ok(())
}

async fn reconcile_customer(
    config: &Config,
    stripe: &StripeClient,
    pool: &PgPool,
    customer_id: &str,
    since_unix: i64,
) -> anyhow::Result<()> {
    let pis = list_succeeded_auto_topup_pis(stripe, customer_id, since_unix).await?;
    for pi in pis {
        let pi_id = match pi.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let amount = pi.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
        if amount <= 0 {
            continue;
        }
        // Pass A (repair_partial_credits) already handles every row with
        // an existing `auto_topup_credits` entry, so Pass B only needs
        // to act when there is no row at all — the "PI succeeded at
        // Stripe but the service crashed before INSERT" path.
        if db::find_credit_row(pool, &pi_id).await?.is_some() {
            continue;
        }
        tracing::warn!(customer_id, pi_id, "reconciler: applying orphaned credit");
        let inserted = db::try_insert_credit(pool, &pi_id, customer_id, amount).await?;
        if !inserted {
            continue;
        }
        let bt_id = write_balance_transaction(stripe, customer_id, &pi_id, amount).await?;
        db::mark_credit_completed(pool, &pi_id, &bt_id).await?;
        let _ = invalidate_cache(config, customer_id).await;
    }
    Ok(())
}

async fn list_succeeded_auto_topup_pis(
    stripe: &StripeClient,
    customer_id: &str,
    since_unix: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut starting_after: Option<String> = None;
    let since_str = since_unix.to_string();
    loop {
        let mut params: Vec<(&str, &str)> = vec![
            ("customer", customer_id),
            ("limit", "100"),
            ("created[gte]", since_str.as_str()),
        ];
        if let Some(ref s) = starting_after {
            params.push(("starting_after", s.as_str()));
        }
        let resp = stripe
            .get("payment_intents", &params)
            .await
            .context("payment_intents.list (reconciler)")?;
        let arr = match resp.body.get("data").and_then(|d| d.as_array()) {
            Some(arr) => arr.clone(),
            None => break,
        };
        let has_more = resp
            .body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut last_id: Option<String> = None;
        for pi in arr.iter() {
            let is_auto =
                pi.pointer("/metadata/source").and_then(|v| v.as_str()) == Some("auto_topup");
            let succeeded = pi.get("status").and_then(|v| v.as_str()) == Some("succeeded");
            if is_auto && succeeded {
                out.push(pi.clone());
            }
            last_id = pi.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if !has_more {
            break;
        }
        match last_id {
            Some(id) => starting_after = Some(id),
            None => break,
        }
    }
    Ok(out)
}

async fn write_balance_transaction(
    stripe: &StripeClient,
    customer_id: &str,
    pi_id: &str,
    amount_cents: i64,
) -> anyhow::Result<String> {
    let credit_idem = format!("credit:{pi_id}");
    let neg = (-amount_cents).to_string();
    let resp = stripe
        .post_with_idempotency(
            &format!("customers/{customer_id}/balance_transactions"),
            &[
                ("amount", neg.as_str()),
                ("currency", "usd"),
                ("description", &format!("Auto top-up via {pi_id}")),
            ],
            &credit_idem,
        )
        .await
        .context("reconciler balance_transactions write")?;
    let id = resp
        .body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("balance_tx response missing id"))?
        .to_string();
    Ok(id)
}

async fn invalidate_cache(config: &Config, customer_id: &str) -> anyhow::Result<()> {
    let client = internal_client::build_client()?;
    let body = serde_json::json!({ "customer_id": customer_id });
    internal_client::post_internal(&client, config, "/internal/invalidate_balance_cache", &body)
        .await
}
