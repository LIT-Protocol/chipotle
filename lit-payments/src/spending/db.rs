//! Postgres queries for spending rules + rolling usage.
//!
//! Runtime `sqlx` (no compile-time DB), matching the rest of the service.

use anyhow::Result;
use sqlx::PgPool;

use super::types::{SpendingRules, SpendingUsage, UpsertRulesRequest};

/// Insert or replace the rules for a key, returning the stored row.
pub async fn upsert_rules(
    pool: &PgPool,
    api_key_hash: &str,
    req: &UpsertRulesRequest,
) -> Result<SpendingRules> {
    let row = sqlx::query_as::<_, SpendingRules>(
        "INSERT INTO spending_rules (
             api_key_hash, account_wallet_address, spend_cap_cents, spend_window_seconds,
             rate_limit_rps, rate_limit_burst, max_concurrency, allowed_origins, enabled, updated_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
         ON CONFLICT (api_key_hash) DO UPDATE SET
             account_wallet_address = EXCLUDED.account_wallet_address,
             spend_cap_cents        = EXCLUDED.spend_cap_cents,
             spend_window_seconds   = EXCLUDED.spend_window_seconds,
             rate_limit_rps         = EXCLUDED.rate_limit_rps,
             rate_limit_burst       = EXCLUDED.rate_limit_burst,
             max_concurrency        = EXCLUDED.max_concurrency,
             allowed_origins        = EXCLUDED.allowed_origins,
             enabled                = EXCLUDED.enabled,
             updated_at             = now()
         RETURNING *",
    )
    .bind(api_key_hash)
    .bind(req.account_wallet_address.as_deref())
    .bind(req.spend_cap_cents)
    .bind(req.spend_window_seconds)
    .bind(req.rate_limit_rps)
    .bind(req.rate_limit_burst)
    .bind(req.max_concurrency)
    .bind(req.allowed_origins.as_deref())
    .bind(req.enabled)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_rules(pool: &PgPool, api_key_hash: &str) -> Result<Option<SpendingRules>> {
    let row = sqlx::query_as::<_, SpendingRules>(
        "SELECT * FROM spending_rules WHERE api_key_hash = $1",
    )
    .bind(api_key_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_usage(pool: &PgPool, api_key_hash: &str) -> Result<Option<SpendingUsage>> {
    let row = sqlx::query_as::<_, SpendingUsage>(
        "SELECT * FROM spending_usage WHERE api_key_hash = $1",
    )
    .bind(api_key_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list_rules(pool: &PgPool, limit: i64) -> Result<Vec<SpendingRules>> {
    let rows = sqlx::query_as::<_, SpendingRules>(
        "SELECT * FROM spending_rules ORDER BY updated_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete a key's rules and its usage counter. Returns whether a rules row existed.
pub async fn delete_rules(pool: &PgPool, api_key_hash: &str) -> Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM spending_usage WHERE api_key_hash = $1")
        .bind(api_key_hash)
        .execute(&mut *tx)
        .await?;
    let res = sqlx::query("DELETE FROM spending_rules WHERE api_key_hash = $1")
        .bind(api_key_hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Add `cents` to a key's rolling spend counter, resetting the window first if
/// it has elapsed. One atomic statement so concurrent charges can't race the
/// read-modify-write. Returns the post-charge counter.
pub async fn record_charge(
    pool: &PgPool,
    api_key_hash: &str,
    cents: i64,
    window_seconds: i64,
) -> Result<SpendingUsage> {
    let row = sqlx::query_as::<_, SpendingUsage>(
        "INSERT INTO spending_usage AS u (api_key_hash, window_started_at, spent_cents, updated_at)
         VALUES ($1, now(), $2, now())
         ON CONFLICT (api_key_hash) DO UPDATE SET
             window_started_at = CASE
                 WHEN now() - u.window_started_at >= make_interval(secs => $3)
                 THEN now() ELSE u.window_started_at END,
             spent_cents = CASE
                 WHEN now() - u.window_started_at >= make_interval(secs => $3)
                 THEN $2 ELSE u.spent_cents + $2 END,
             updated_at = now()
         RETURNING *",
    )
    .bind(api_key_hash)
    .bind(cents)
    .bind(window_seconds as f64)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
