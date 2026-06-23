//! Postgres helpers for the gas funder.
//!
//! Wei amounts are stored in NUMERIC columns and crossed the sqlx boundary as
//! base-10 strings (cast `$n::numeric` on write, `::text` on read) so we never
//! round a 256-bit value through an `f64`. The 24h cap sum and the alert
//! cooldown both run as single statements — the funder loop is single-instance
//! (Railway `numReplicas: 1`), so there's no cross-replica race to guard.

use alloy_primitives::U256;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::str::FromStr;

/// Sum of all non-failed funding amounts in the last 24h for `chain_id`.
/// Backs the rolling daily spend cap. `pending` rows are included so an
/// interrupted send still consumes budget (conservative).
pub async fn funded_last_24h(pool: &PgPool, chain_id: i64) -> Result<U256> {
    let total: String = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_wei), 0)::text
           FROM gas_funding_events
          WHERE chain_id = $1
            AND status <> 'failed'
            AND created_at > now() - interval '24 hours'",
    )
    .bind(chain_id)
    .fetch_one(pool)
    .await
    .context("summing 24h gas funding")?;
    U256::from_str(total.trim()).with_context(|| format!("parsing 24h funding sum {total:?}"))
}

/// Record an intended send as `pending` BEFORE broadcasting. Returns the row
/// id so the caller can transition it to `sent`/`failed`.
pub async fn insert_pending(
    pool: &PgPool,
    chain_id: i64,
    recipient: &str,
    amount_wei: &str,
    balance_before_wei: &str,
) -> Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO gas_funding_events
            (chain_id, recipient, amount_wei, balance_before_wei, status)
         VALUES ($1, $2, $3::numeric, $4::numeric, 'pending')
         RETURNING id",
    )
    .bind(chain_id)
    .bind(recipient)
    .bind(amount_wei)
    .bind(balance_before_wei)
    .fetch_one(pool)
    .await
    .context("insert pending gas funding event")?;
    Ok(id)
}

pub async fn mark_sent(pool: &PgPool, id: i64, tx_hash: &str) -> Result<()> {
    sqlx::query(
        "UPDATE gas_funding_events
            SET status = 'sent', tx_hash = $2, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(tx_hash)
    .execute(pool)
    .await
    .context("mark gas funding event sent")?;
    Ok(())
}

pub async fn mark_failed(pool: &PgPool, id: i64, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE gas_funding_events
            SET status = 'failed', error = $2, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await
    .context("mark gas funding event failed")?;
    Ok(())
}

/// Count `pending` rows older than `older_than_secs` — these are sends that
/// were recorded but never transitioned (a crash between insert and receipt).
/// Surfaced as a warning; never auto-retried (that could double-spend).
pub async fn count_stale_pending(pool: &PgPool, older_than_secs: i64) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM gas_funding_events
          WHERE status = 'pending'
            AND created_at < now() - ($1 || ' seconds')::interval",
    )
    .bind(older_than_secs.to_string())
    .fetch_one(pool)
    .await
    .context("count stale pending gas funding events")?;
    Ok(n)
}

/// Acquire the right to send the alert keyed by `key`, respecting a cooldown.
///
/// Returns `true` if no alert for this key has been sent within
/// `cooldown_secs` (and stamps `now()`), `false` if it's still cooling down.
/// The decision and the stamp happen in one upsert so repeated ticks can't
/// both pass.
pub async fn should_alert(pool: &PgPool, key: &str, cooldown_secs: i64) -> Result<bool> {
    let acquired: Option<String> = sqlx::query_scalar(
        "INSERT INTO gas_funder_alerts (alert_key, last_sent_at)
         VALUES ($1, now())
         ON CONFLICT (alert_key) DO UPDATE
            SET last_sent_at = now()
          WHERE gas_funder_alerts.last_sent_at < now() - ($2 || ' seconds')::interval
         RETURNING alert_key",
    )
    .bind(key)
    .bind(cooldown_secs.to_string())
    .fetch_optional(pool)
    .await
    .context("gas funder alert cooldown upsert")?;
    Ok(acquired.is_some())
}
