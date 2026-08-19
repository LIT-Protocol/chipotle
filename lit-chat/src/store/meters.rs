use anyhow::Result;
use sqlx::PgPool;

/// Load the user's encrypted meter blob (and its version for CAS).
pub async fn get(pool: &PgPool, user_ref_hash: &str) -> Result<Option<(Vec<u8>, i64)>> {
    let row: Option<(Vec<u8>, i64)> =
        sqlx::query_as("SELECT enc_meter, version FROM user_meters WHERE user_ref_hash = $1")
            .bind(user_ref_hash)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// Compare-and-swap write. Returns false on version conflict (caller
/// re-reads and retries).
pub async fn put(
    pool: &PgPool,
    user_ref_hash: &str,
    enc_meter: &[u8],
    expected_version: Option<i64>,
) -> Result<bool> {
    match expected_version {
        None => {
            let res = sqlx::query(
                "INSERT INTO user_meters (user_ref_hash, enc_meter) VALUES ($1, $2)
                 ON CONFLICT (user_ref_hash) DO NOTHING",
            )
            .bind(user_ref_hash)
            .bind(enc_meter)
            .execute(pool)
            .await?;
            Ok(res.rows_affected() > 0)
        }
        Some(v) => {
            let res = sqlx::query(
                "UPDATE user_meters
                 SET enc_meter = $1, version = version + 1, updated_at = now()
                 WHERE user_ref_hash = $2 AND version = $3",
            )
            .bind(enc_meter)
            .bind(user_ref_hash)
            .bind(v)
            .execute(pool)
            .await?;
            Ok(res.rows_affected() > 0)
        }
    }
}

/// Aggregate, non-attributable daily counters (breaker input + ops/finance
/// visibility). Plaintext by design: day totals only.
pub async fn add_day_spend(pool: &PgPool, day: &str, micro_usd: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO spend_days (day, micro_usd, requests) VALUES ($1, $2, 1)
         ON CONFLICT (day) DO UPDATE
         SET micro_usd = spend_days.micro_usd + EXCLUDED.micro_usd,
             requests = spend_days.requests + 1",
    )
    .bind(day)
    .bind(micro_usd)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn day_spend(pool: &PgPool, day: &str) -> Result<i64> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT micro_usd FROM spend_days WHERE day = $1")
        .bind(day)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(v,)| v).unwrap_or(0))
}

pub async fn recent_days(pool: &PgPool, limit: i64) -> Result<Vec<(String, i64, i64)>> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT day, micro_usd, requests FROM spend_days ORDER BY day DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
