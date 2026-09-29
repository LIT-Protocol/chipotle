//! Agent access tokens authorized by a logged-in user.
//!
//! A binding (`token_hash -> user_id`) is only ever created through the pairing
//! flow in [`super::pairing`]: the agent registers its token hash server-side,
//! the user approves the resulting single-use code in their browser, and the
//! agent proves possession of the raw token. The browser never chooses the
//! binding target, and an existing binding is never moved to a different user or
//! silently un-revoked.

use anyhow::{Context, Result};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::token as auth_token;

const MIN_AGENT_TOKEN_LEN: usize = 32;
const MAX_AGENT_TOKEN_LEN: usize = 512;

const TOKEN_HASH_LEN: usize = 43;

/// Agent tokens expire 90 days after they are bound. A forgotten or leaked token
/// stops working on its own instead of granting indefinite access.
pub const AGENT_TOKEN_TTL_SECONDS: i64 = 90 * 24 * 60 * 60;

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

/// Bind `token_hash` to `user_id` with an expiry.
///
/// The `ON CONFLICT` clause is deliberately guarded with
/// `WHERE agent_access_tokens.user_id = EXCLUDED.user_id`: an existing binding is
/// only ever refreshed for the user who already owns it. A conflicting hash owned
/// by a different user is left untouched (no `user_id` move, no `revoked_at`
/// clear), which is what closes the cross-account rebind primitive.
///
/// The caller must have validated that whoever is being bound both (a) had the
/// pairing approved by the account owner and (b) proved possession of the raw
/// token preimage — see [`super::pairing::complete`].
/// Returns the number of rows written. `0` means a conflicting hash is already
/// owned by a *different* user (the guarded `ON CONFLICT` was a no-op) — the
/// caller must not treat that as a successful bind.
pub async fn bind_token(
    pool: &PgPool,
    token_hash: &str,
    user_id: Uuid,
    label: Option<&str>,
) -> Result<u64> {
    validate_agent_token_hash(token_hash)?;
    let label = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local-agent");
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(AGENT_TOKEN_TTL_SECONDS);

    let result = sqlx::query(
        "INSERT INTO agent_access_tokens (token_hash, user_id, label, expires_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (token_hash) DO UPDATE
           SET label = EXCLUDED.label,
               expires_at = EXCLUDED.expires_at,
               revoked_at = NULL
         WHERE agent_access_tokens.user_id = EXCLUDED.user_id",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(label)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn lookup(pool: &PgPool, raw_token: &str) -> Result<Option<Uuid>> {
    validate_agent_token(raw_token).context("invalid agent bearer token")?;
    let row = sqlx::query_as::<_, (Uuid,)>(
        "UPDATE agent_access_tokens
         SET last_used_at = now()
         WHERE token_hash = $1
           AND revoked_at IS NULL
           AND (expires_at IS NULL OR expires_at > now())
         RETURNING user_id",
    )
    .bind(auth_token::token_hash(raw_token))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(user_id,)| user_id))
}

/// Self-revoke: revoke the token whose raw value is presented. Requires no user
/// context because holding the raw token is itself proof of ownership.
pub async fn revoke_by_token(pool: &PgPool, raw_token: &str) -> Result<u64> {
    validate_agent_token(raw_token)?;
    let result = sqlx::query(
        "UPDATE agent_access_tokens
         SET revoked_at = now()
         WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(auth_token::token_hash(raw_token))
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Revoke every active agent token owned by `user_id`. Lets a signed-in user cut
/// off all agents from the browser, even without holding any raw token.
pub async fn revoke_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE agent_access_tokens
         SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
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
    fn validates_token_hash_shape() {
        // 43 chars is the base64url-no-pad length of a sha256 digest.
        let ok = "A".repeat(TOKEN_HASH_LEN);
        assert!(validate_agent_token_hash(&ok).is_ok());
        assert!(validate_agent_token_hash("tooshort").is_err());
        let bad_char = format!("{}+", "A".repeat(TOKEN_HASH_LEN - 1));
        assert!(validate_agent_token_hash(&bad_char).is_err());
    }

    #[test]
    fn sanitize_next_path_allows_only_local_paths() {
        assert_eq!(
            sanitize_next_path(Some("/agent/authorize?code=abc")),
            Some("/agent/authorize?code=abc".to_string())
        );
        assert_eq!(sanitize_next_path(Some("https://evil.test")), None);
        assert_eq!(sanitize_next_path(Some("//evil.test/path")), None);
        assert_eq!(sanitize_next_path(Some("/ok\nLocation: /evil")), None);
    }
}
