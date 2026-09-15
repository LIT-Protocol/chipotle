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
use sqlx::pool::PoolConnection;
use sqlx::postgres::Postgres;
use std::str::FromStr;

/// Postgres session advisory-lock key for the funder singleton. Guards against
/// two funder processes running concurrently (e.g. during a Railway deploy
/// overlap, where the new instance boots and fires its first tick before the
/// old one is torn down) — `numReplicas: 1` does not cover that window.
const GAS_FUNDER_LOCK_KEY: i64 = 7_766_980_010_001;

/// Try to take the funder's singleton advisory lock on a dedicated pooled
/// connection. Returns the held connection on success (caller MUST pass it to
/// [`release_lock`] when done), or `None` if another process holds it.
///
/// The lock is a *session* lock bound to this connection, so the connection is
/// held for the whole tick and explicitly unlocked before being returned to
/// the pool — dropping it without unlocking would strand the lock on a pooled
/// backend.
pub async fn try_acquire_lock(pool: &PgPool) -> Result<Option<PoolConnection<Postgres>>> {
    let mut conn = pool
        .acquire()
        .await
        .context("acquiring connection for funder advisory lock")?;
    let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(GAS_FUNDER_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await
        .context("pg_try_advisory_lock")?;
    if got { Ok(Some(conn)) } else { Ok(None) }
}

/// Release the funder advisory lock and return the connection to the pool.
pub async fn release_lock(mut conn: PoolConnection<Postgres>) {
    if let Err(e) = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(GAS_FUNDER_LOCK_KEY)
        .execute(&mut *conn)
        .await
    {
        tracing::warn!("gas_funder: failed to release advisory lock: {e}");
    }
}

/// Sum of all non-failed funding amounts in the last 24h for `chain_id`.
/// Backs the rolling daily spend cap. `pending` and `broadcast` rows are
/// included so an interrupted or unconfirmed send still consumes budget
/// (fail-closed: we never refund budget for money that may be in flight).
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

/// Record that the transaction was accepted by the RPC (broadcast) and stamp
/// its hash, BEFORE we wait for the receipt. This is the critical fail-closed
/// step: if the subsequent receipt wait times out or errors, the row stays
/// `broadcast` (still counted against the cap) and carries the hash, so we
/// never "forget" money already in the mempool and re-send it.
pub async fn mark_broadcast(pool: &PgPool, id: i64, tx_hash: &str) -> Result<()> {
    sqlx::query(
        "UPDATE gas_funding_events
            SET status = 'broadcast', tx_hash = $2, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(tx_hash)
    .execute(pool)
    .await
    .context("mark gas funding event broadcast")?;
    Ok(())
}

/// True if `recipient` already has a non-failed funding row within
/// `within_secs`. Used to skip re-funding a wallet whose previous top-up is
/// still in flight (or just landed), preventing a double-send when a tx is
/// slow to mine.
pub async fn funded_recipient_recently(
    pool: &PgPool,
    chain_id: i64,
    recipient: &str,
    within_secs: i64,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM gas_funding_events
             WHERE chain_id = $1
               AND recipient = $2
               AND status <> 'failed'
               AND created_at > now() - ($3 || ' seconds')::interval)",
    )
    .bind(chain_id)
    .bind(recipient)
    .bind(within_secs.to_string())
    .fetch_one(pool)
    .await
    .context("checking recent funding for recipient")?;
    Ok(exists)
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

/// Count unresolved rows (`pending` recorded-but-not-broadcast, or `broadcast`
/// awaiting a receipt we never observed) older than `older_than_secs`. These
/// are interrupted/unconfirmed sends. Surfaced as a warning; never auto-retried
/// (that could double-spend).
pub async fn count_stale_pending(pool: &PgPool, older_than_secs: i64) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM gas_funding_events
          WHERE status IN ('pending', 'broadcast')
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

/// Clear an alert's cooldown so it can fire again on the next tick. Called when
/// the email send fails *after* `should_alert` already stamped the cooldown, so
/// a failed notification doesn't silence the alert for the whole window.
pub async fn reset_alert(pool: &PgPool, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM gas_funder_alerts WHERE alert_key = $1")
        .bind(key)
        .execute(pool)
        .await
        .context("resetting gas funder alert cooldown")?;
    Ok(())
}
