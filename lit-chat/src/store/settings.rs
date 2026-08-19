use anyhow::Result;
use sqlx::PgPool;

pub const BREAKER_KEY: &str = "breaker";
pub const CAPS_KEY: &str = "caps";

/// Circuit-breaker state (section 4.2): degrades to "account holders only"
/// before hard-off. `auto` follows the daily cap; the admin console can force
/// a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakerMode {
    Auto,
    ForcedAccountsOnly,
    ForcedOff,
    ForcedOn,
}

pub async fn get(pool: &PgPool, key: &str) -> Result<Option<(String, i64)>> {
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT value, version FROM settings WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

pub async fn put(pool: &PgPool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE
         SET value = EXCLUDED.value, version = settings.version + 1, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn breaker_mode(pool: &PgPool) -> Result<BreakerMode> {
    match get(pool, BREAKER_KEY).await? {
        Some((v, _)) => Ok(serde_json::from_str(&v).unwrap_or(BreakerMode::Auto)),
        None => Ok(BreakerMode::Auto),
    }
}

/// Runtime-tunable caps; falls back to the deploy-time config when unset.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Caps {
    pub daily_spend_cap_micro_usd: i64,
    pub anon_daily_token_budget: i64,
}

pub async fn caps(pool: &PgPool, defaults: &Caps) -> Result<Caps> {
    match get(pool, CAPS_KEY).await? {
        Some((v, _)) => Ok(serde_json::from_str(&v).unwrap_or_else(|_| defaults.clone())),
        None => Ok(defaults.clone()),
    }
}
