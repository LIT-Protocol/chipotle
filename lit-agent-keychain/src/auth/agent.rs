//! Agent access tokens authorized by a logged-in user.

use anyhow::{Context, Result};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::token as auth_token;

const MIN_AGENT_TOKEN_LEN: usize = 32;
const MAX_AGENT_TOKEN_LEN: usize = 512;

const TOKEN_HASH_LEN: usize = 43;

pub fn validate_agent_token(token: &str) -> Result<()> {
    let len = token.len();
    if !(MIN_AGENT_TOKEN_LEN..=MAX_AGENT_TOKEN_LEN).contains(&len) {
        anyhow::bail!("agent token must be 32-512 characters");
    }
    if !token
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
    {
        anyhow::bail!("agent token contains unsupported characters");
    }
    Ok(())
}

pub fn validate_agent_token_hash(token_hash: &str) -> Result<()> {
    if token_hash.len() != TOKEN_HASH_LEN {
        anyhow::bail!("agent token challenge has invalid length");
    }
    if !token_hash
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        anyhow::bail!("agent token challenge contains unsupported characters");
    }
    Ok(())
}

pub async fn authorize_hash(
    pool: &PgPool,
    token_hash: &str,
    user_id: Uuid,
    label: Option<&str>,
) -> Result<()> {
    validate_agent_token_hash(token_hash)?;
    let label: String = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local-agent")
        .chars()
        .take(64)
        .collect();

    // A token hash may only ever be (re-)bound by the user it already belongs
    // to. Without the WHERE guard, an attacker who obtains a victim's
    // authorize URL could submit its challenge while logged in as themselves,
    // silently re-pointing the victim's setup token at the attacker's account
    // (codex finding, 2026-09-10). Same-user re-authorization (including
    // un-revoking) stays allowed.
    let result = sqlx::query(
        "INSERT INTO agent_access_tokens (token_hash, user_id, label)
         VALUES ($1, $2, $3)
         ON CONFLICT (token_hash) DO UPDATE
           SET label = EXCLUDED.label,
               revoked_at = NULL
         WHERE agent_access_tokens.user_id = EXCLUDED.user_id",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(label)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        anyhow::bail!("agent token is already bound to a different user");
    }
    Ok(())
}

pub async fn authorize(
    pool: &PgPool,
    raw_token: &str,
    user_id: Uuid,
    label: Option<&str>,
) -> Result<()> {
    validate_agent_token(raw_token)?;
    let token_hash = auth_token::token_hash(raw_token);
    authorize_hash(pool, &token_hash, user_id, label).await
}

pub async fn lookup(pool: &PgPool, raw_token: &str) -> Result<Option<Uuid>> {
    validate_agent_token(raw_token).context("invalid agent bearer token")?;
    let row = sqlx::query_as::<_, (Uuid,)>(
        "UPDATE agent_access_tokens
         SET last_used_at = now()
         WHERE token_hash = $1 AND revoked_at IS NULL
         RETURNING user_id",
    )
    .bind(auth_token::token_hash(raw_token))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(user_id,)| user_id))
}

/// A setup-agent authorization as shown to its owner. `id` is the token hash
/// (never the token), which is what `/agent/authorize?challenge=` carried.
#[derive(Debug, serde::Serialize)]
pub struct SetupToken {
    pub id: String,
    pub label: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
}

pub async fn list(pool: &PgPool, user_id: Uuid) -> Result<Vec<SetupToken>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            OffsetDateTime,
            Option<OffsetDateTime>,
            Option<OffsetDateTime>,
        ),
    >(
        "SELECT token_hash, label, created_at, last_used_at, revoked_at
         FROM agent_access_tokens WHERE user_id = $1
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, label, created_at, last_used_at, revoked_at)| SetupToken {
                id,
                label,
                created_at,
                last_used_at,
                revoked_at,
            },
        )
        .collect())
}

/// Revoke by token hash (the id shown in the dashboard). Returns rows affected:
/// 0 if the hash is unknown, belongs to someone else, or is already revoked.
pub async fn revoke_hash(pool: &PgPool, token_hash: &str, user_id: Uuid) -> Result<u64> {
    validate_agent_token_hash(token_hash)?;
    let result = sqlx::query(
        "UPDATE agent_access_tokens
         SET revoked_at = now()
         WHERE token_hash = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(token_hash)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub fn sanitize_next_path(next: Option<&str>) -> Option<String> {
    let next = next?.trim();
    if !next.starts_with('/')
        || next.starts_with("//")
        || next.contains('\n')
        || next.contains('\r')
    {
        return None;
    }
    Some(next.chars().take(512).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_url_safe_agent_tokens() {
        assert!(validate_agent_token("abcdefghijklmnopqrstuvwxyzABCDEF").is_ok());
        assert!(validate_agent_token("abcDEF0123456789-_~.abcDEF0123456789").is_ok());
        assert!(validate_agent_token("short").is_err());
        assert!(validate_agent_token("abcdefghijklmnopqrstuvwxyzABCDE+").is_err());
    }

    #[test]
    fn sanitize_next_path_allows_only_local_paths() {
        assert_eq!(
            sanitize_next_path(Some("/agent/authorize?challenge=abc")),
            Some("/agent/authorize?challenge=abc".to_string())
        );
        assert_eq!(sanitize_next_path(Some("https://evil.test")), None);
        assert_eq!(sanitize_next_path(Some("//evil.test/path")), None);
        assert_eq!(sanitize_next_path(Some("/ok\nLocation: /evil")), None);
    }
}
