//! Auto top-up reconciler.
//!
//! Plan §6 / §9: the sync-credit webhook handler can fail mid-flow:
//!   - HTTP timeout on `paymentIntents.create` — the PI may still
//!     succeed at Stripe but we never see the response.
//!   - DB write timeout between `INSERT auto_topup_credits` and
//!     `balance_transactions` — credit row exists, balance not credited.
//!   - Process crash anywhere in the middle.
//!
//! The reconciler closes these gaps by sweeping each enabled customer's
//! recent auto_topup PIs (last 7 days) every `RECONCILER_INTERVAL_SECS`
//! and applying any missing credit work. The PI ledger
//! (auto_topup_credits) + Stripe Idempotency-Key `credit:{pi.id}` make
//! the sweep safe to run concurrently with the live webhook handler.
//!
//! Spawned from main.rs at startup; runs for the lifetime of the
//! process. The process exits non-cleanly on shutdown (no graceful
//! drain) — that's fine because the next reconciler tick on the
//! restarted process picks up any work that was interrupted.

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

/// Single sweep across every enabled customer. Public for tests; the
/// production scheduler in [`spawn`] is the only other caller.
pub async fn run_once(config: &Config, stripe: &StripeClient, pool: &PgPool) -> anyhow::Result<()> {
    let customers = db::list_enabled_customers(pool)
        .await
        .context("list enabled customers")?;
    if customers.is_empty() {
        return Ok(());
    }
    let since_unix =
        (OffsetDateTime::now_utc() - ::time::Duration::days(LOOKBACK_DAYS)).unix_timestamp();
    for customer_id in customers {
        if let Err(e) = reconcile_customer(config, stripe, pool, &customer_id, since_unix).await {
            tracing::warn!(customer_id, "reconcile_customer failed: {e:?}");
        }
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
        match db::find_credit_row(pool, &pi_id).await? {
            Some(row) if row.stripe_balance_transaction_id.is_some() => {
                // Credited. Skip.
            }
            Some(_partial) => {
                // Row exists but balance_tx never landed. Retry just the
                // balance_transactions write (with the same idempotency
                // key — Stripe dedupes if the prior attempt actually
                // succeeded).
                tracing::warn!(customer_id, pi_id, "reconciler: completing partial credit");
                let bt_id = write_balance_transaction(stripe, customer_id, &pi_id, amount).await?;
                db::mark_credit_completed(pool, &pi_id, &bt_id).await?;
                let _ = invalidate_cache(config, customer_id).await;
            }
            None => {
                // No DB row at all — webhook handler never reached the
                // INSERT (process crash, HTTP timeout on PI create, or
                // mid-air interruption). Do the full credit dance.
                tracing::warn!(customer_id, pi_id, "reconciler: applying orphaned credit");
                let inserted = db::try_insert_credit(pool, &pi_id, customer_id, amount).await?;
                if !inserted {
                    // Lost the race with the webhook handler — fine.
                    continue;
                }
                let bt_id = write_balance_transaction(stripe, customer_id, &pi_id, amount).await?;
                db::mark_credit_completed(pool, &pi_id, &bt_id).await?;
                let _ = invalidate_cache(config, customer_id).await;
            }
        }
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
