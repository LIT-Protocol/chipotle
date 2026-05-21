//! Session token issuing + the Rocket request guard that loads an
//! authenticated [`Operator`] from a session cookie.

use anyhow::Result;
use base64::Engine;
use rand::RngCore;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use sqlx::PgPool;
use time::OffsetDateTime;

use super::operator::{self, Operator};
use super::{SESSION_COOKIE_NAME, SESSION_TTL_SECONDS};

/// Generate a random 32-byte URL-safe session token.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Insert a new session row.
pub async fn create(pool: &PgPool, token: &str, operator_id: i64) -> Result<()> {
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(SESSION_TTL_SECONDS);
    sqlx::query("INSERT INTO sessions (token, operator_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(operator_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Look up an active session by token. Returns `Ok(None)` for unknown or
/// expired sessions.
pub async fn lookup(pool: &PgPool, token: &str) -> Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64, OffsetDateTime)>(
        "SELECT operator_id, expires_at FROM sessions WHERE token = $1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    let Some((operator_id, expires_at)) = row else {
        return Ok(None);
    };
    if expires_at <= OffsetDateTime::now_utc() {
        return Ok(None);
    }
    Ok(Some(operator_id))
}

/// Delete a session row. Idempotent.
pub async fn delete(pool: &PgPool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

/// Best-effort cleanup of expired sessions. Called occasionally; not on
/// every request.
pub async fn purge_expired(pool: &PgPool) -> Result<u64> {
    let r = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// Request guard: extracts the session cookie, looks up the session and
/// operator, and yields an [`Operator`]. Forwards (404 to next handler) if
/// no cookie or no valid session — let the route decide whether to redirect
/// to /login or 401.
#[rocket::async_trait]
impl<'r> FromRequest<'r> for Operator {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(cookie) = req.cookies().get_private(SESSION_COOKIE_NAME) else {
            return Outcome::Forward(Status::Unauthorized);
        };
        let token = cookie.value().to_string();

        let pool = match req.rocket().state::<PgPool>() {
            Some(p) => p,
            None => {
                tracing::error!("Operator guard: PgPool not in Rocket state");
                return Outcome::Error((Status::InternalServerError, ()));
            }
        };

        let operator_id = match lookup(pool, &token).await {
            Ok(Some(id)) => id,
            Ok(None) => return Outcome::Forward(Status::Unauthorized),
            Err(e) => {
                tracing::warn!("session lookup failed: {e}");
                return Outcome::Error((Status::InternalServerError, ()));
            }
        };

        match operator::find_by_id(pool, operator_id).await {
            Ok(Some(op)) => Outcome::Success(op),
            Ok(None) => {
                // Operator deleted while their session is still active — treat
                // as unauthenticated.
                Outcome::Forward(Status::Unauthorized)
            }
            Err(e) => {
                tracing::warn!("operator lookup failed: {e}");
                Outcome::Error((Status::InternalServerError, ()))
            }
        }
    }
}
