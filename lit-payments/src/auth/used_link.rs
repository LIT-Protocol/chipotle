//! Single-use enforcement for magic-link tokens (CPL-379 L8).
//!
//! Magic-link tokens are stateless HMAC-signed blobs (see [`super::token`]), so
//! signature + expiry alone let the same link be replayed for its full TTL.
//! This module records each token's hash the first time it is redeemed; a
//! second presentation conflicts on the primary key and is rejected.

use anyhow::Result;
use sqlx::{PgExecutor, PgPool};
use time::OffsetDateTime;

/// Atomically record a magic-link token as consumed. Returns `true` if this is
/// the first time it has been redeemed (the caller may proceed to log in), or
/// `false` if it was already used (the caller must reject the replay).
///
/// `token_hash` is the SHA-256 hex of the raw token (see [`super::token::token_hash`])
/// — the raw token is never persisted. `expires_at` is the token's own expiry,
/// stored only so consumed rows can be purged once the token would expire on
/// its own. The `INSERT ... ON CONFLICT DO NOTHING RETURNING` is a single
/// atomic statement, so concurrent redemptions of the same token race safely:
/// exactly one sees the insert (`true`), the rest see the conflict (`false`).
///
/// Takes any `PgExecutor` so the caller can run this inside the transaction
/// that also creates the session; if that later insert fails the whole
/// transaction rolls back and the token is left unconsumed (CPL-379 L8).
pub async fn try_consume(
    executor: impl PgExecutor<'_>,
    token_hash: &str,
    email: &str,
    expires_at: OffsetDateTime,
) -> Result<bool> {
    let inserted: Option<(String,)> = sqlx::query_as(
        "INSERT INTO used_magic_links (token_hash, email, expires_at) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (token_hash) DO NOTHING \
         RETURNING token_hash",
    )
    .bind(token_hash)
    .bind(email)
    .bind(expires_at)
    .fetch_optional(executor)
    .await?;
    Ok(inserted.is_some())
}

/// Best-effort cleanup of consumed tokens past their original expiry. Called
/// occasionally (e.g. on boot), mirroring [`super::session::purge_expired`].
pub async fn purge_expired(pool: &PgPool) -> Result<u64> {
    let r = sqlx::query("DELETE FROM used_magic_links WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}
