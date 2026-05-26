//! Agent access tokens authorized by a logged-in user.

use anyhow::{Context, Result};
use sqlx::PgPool;
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
    let label = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local-agent");

    sqlx::query(
        "INSERT INTO agent_access_tokens (token_hash, user_id, label)
         VALUES ($1, $2, $3)
         ON CONFLICT (token_hash) DO UPDATE
           SET user_id = EXCLUDED.user_id,
               label = EXCLUDED.label,
               revoked_at = NULL",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(label)
    .execute(pool)
    .await?;
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

#[allow(dead_code)]
pub async fn revoke(pool: &PgPool, raw_token: &str, user_id: Uuid) -> Result<u64> {
    validate_agent_token(raw_token)?;
    let result = sqlx::query(
        "UPDATE agent_access_tokens
         SET revoked_at = now()
         WHERE token_hash = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(auth_token::token_hash(raw_token))
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
