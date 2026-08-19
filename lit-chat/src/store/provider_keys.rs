//! Encrypted provider-key custody (plans/tee-chat-app.md section 4.4).
//!
//! Keys are ciphertext under the service KEK with AAD (key_id, provider,
//! kind). Write-only: plaintext crosses the boundary exactly once (import or
//! mint); every read path below the two `decrypt_*` helpers returns the
//! masked hint only, and there is no reveal endpoint anywhere.

use crate::crypto::aes;
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProviderKeyRow {
    pub id: Uuid,
    pub provider: String,
    pub kind: String,
    pub enc_key: Vec<u8>,
    pub masked_hint: String,
    pub status: String,
    pub spend_limit_usd: Option<f64>,
    pub upstream_hash: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub rotated_at: Option<OffsetDateTime>,
}

const COLS: &str = "id, provider, kind, enc_key, masked_hint, status, spend_limit_usd, \
                    upstream_hash, version, created_by, created_at, rotated_at";

fn aad(id: Uuid, provider: &str, kind: &str) -> String {
    format!("chat.pkey.v1|{id}|{provider}|{kind}")
}

/// Leading 8 + trailing 4 characters — the only representation that ever
/// leaves the enclave.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 12 {
        return "****".to_string();
    }
    let head: String = chars[..8].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}...{tail}")
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &PgPool,
    kek: &[u8; 32],
    provider: &str,
    kind: &str,
    plaintext_key: &str,
    status: &str,
    spend_limit_usd: Option<f64>,
    upstream_hash: Option<&str>,
    created_by: &str,
) -> Result<ProviderKeyRow> {
    let id = Uuid::new_v4();
    let enc_key = aes::encrypt(
        kek,
        plaintext_key.as_bytes(),
        aad(id, provider, kind).as_bytes(),
    )?;
    let row = sqlx::query_as::<_, ProviderKeyRow>(&format!(
        "INSERT INTO provider_keys
           (id, provider, kind, enc_key, masked_hint, status, spend_limit_usd, upstream_hash, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING {COLS}"
    ))
    .bind(id)
    .bind(provider)
    .bind(kind)
    .bind(enc_key)
    .bind(mask(plaintext_key))
    .bind(status)
    .bind(spend_limit_usd)
    .bind(upstream_hash)
    .bind(created_by)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub fn decrypt_key(kek: &[u8; 32], row: &ProviderKeyRow) -> Result<String> {
    let bytes = aes::decrypt(
        kek,
        &row.enc_key,
        aad(row.id, &row.provider, &row.kind).as_bytes(),
    )?;
    String::from_utf8(bytes).map_err(|_| anyhow!("provider key is not utf8"))
}

pub async fn list(pool: &PgPool, provider: &str) -> Result<Vec<ProviderKeyRow>> {
    let rows = sqlx::query_as::<_, ProviderKeyRow>(&format!(
        "SELECT {COLS} FROM provider_keys WHERE provider = $1 ORDER BY created_at DESC"
    ))
    .bind(provider)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<ProviderKeyRow>> {
    let row = sqlx::query_as::<_, ProviderKeyRow>(&format!(
        "SELECT {COLS} FROM provider_keys WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The runtime key lit-chat calls OpenRouter with: newest `active` runtime
/// key. Falls back to `retiring` so a promote/demote race never drops
/// in-flight service.
pub async fn active_runtime(pool: &PgPool, provider: &str) -> Result<Option<ProviderKeyRow>> {
    let row = sqlx::query_as::<_, ProviderKeyRow>(&format!(
        "SELECT {COLS} FROM provider_keys
         WHERE provider = $1 AND kind = 'runtime' AND status IN ('active', 'retiring')
         ORDER BY (status = 'active') DESC, rotated_at DESC NULLS LAST, created_at DESC
         LIMIT 1"
    ))
    .bind(provider)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn provisioning(pool: &PgPool, provider: &str) -> Result<Option<ProviderKeyRow>> {
    let row = sqlx::query_as::<_, ProviderKeyRow>(&format!(
        "SELECT {COLS} FROM provider_keys
         WHERE provider = $1 AND kind = 'provisioning' AND status = 'active'
         ORDER BY created_at DESC LIMIT 1"
    ))
    .bind(provider)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn set_status(pool: &PgPool, id: Uuid, status: &str) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE provider_keys
         SET status = $1, version = version + 1, rotated_at = now()
         WHERE id = $2",
    )
    .bind(status)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Demote any currently-active runtime key to `retiring` (promote flow).
pub async fn demote_active_runtime(pool: &PgPool, provider: &str, except: Uuid) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE provider_keys
         SET status = 'retiring', version = version + 1, rotated_at = now()
         WHERE provider = $1 AND kind = 'runtime' AND status = 'active' AND id <> $2",
    )
    .bind(provider)
    .bind(except)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Max version across the table — the consumer's short-TTL cache key, so key
/// swaps take effect within seconds with no restart.
pub async fn table_version(pool: &PgPool) -> Result<i64> {
    let row: (Option<i64>,) = sqlx::query_as("SELECT max(version) FROM provider_keys")
        .fetch_one(pool)
        .await?;
    Ok(row.0.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_shows_first8_last4_only() {
        let m = mask("sk-or-v1-abcdefghijklmnopqrstuvwxyz");
        assert_eq!(m, "sk-or-v1...wxyz");
        assert!(!m.contains("abcdefghijklmnop"));
    }

    #[test]
    fn short_keys_fully_masked() {
        assert_eq!(mask("short"), "****");
        assert_eq!(mask("sk-123456789"), "****");
    }
}
