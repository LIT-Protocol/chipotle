use anyhow::Result;
use sqlx::PgPool;

pub async fn upsert(pool: &PgPool, user_ref_hash: &str, kind: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO chat_users (user_ref_hash, kind) VALUES ($1, $2)
         ON CONFLICT (user_ref_hash) DO NOTHING",
    )
    .bind(user_ref_hash)
    .bind(kind)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn exists(pool: &PgPool, user_ref_hash: &str) -> Result<bool> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT user_ref_hash FROM chat_users WHERE user_ref_hash = $1")
            .bind(user_ref_hash)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// Hard delete: user row cascades to conversations, messages, and meters.
/// No tombstones (plans/tee-chat-app.md section 4.3 deletion honesty).
pub async fn delete(pool: &PgPool, user_ref_hash: &str) -> Result<()> {
    sqlx::query("DELETE FROM chat_users WHERE user_ref_hash = $1")
        .bind(user_ref_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn revoke_session(pool: &PgPool, token_hash: &str) -> Result<()> {
    sqlx::query("INSERT INTO sessions_revoked (token_hash) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn session_revoked(pool: &PgPool, token_hash: &str) -> Result<bool> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT token_hash FROM sessions_revoked WHERE token_hash = $1")
            .bind(token_hash)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// Magic-link replay guard (identity lives in the signed token, never here).
pub async fn store_magic_link(
    pool: &PgPool,
    token_hash: &str,
    expires_at: time::OffsetDateTime,
) -> Result<()> {
    sqlx::query("INSERT INTO magic_links (token_hash, expires_at) VALUES ($1, $2)")
        .bind(token_hash)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Atomic single-use consume. Returns false if unknown, expired, or replayed.
pub async fn consume_magic_link(pool: &PgPool, token_hash: &str) -> Result<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE magic_links SET used_at = now()
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
         RETURNING token_hash",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn purge_expired(pool: &PgPool) -> Result<()> {
    sqlx::query("DELETE FROM magic_links WHERE expires_at <= now() - interval '1 day'")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM sessions_revoked WHERE revoked_at <= now() - interval '200 days'")
        .execute(pool)
        .await?;
    Ok(())
}
