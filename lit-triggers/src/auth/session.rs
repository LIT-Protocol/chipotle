//! Session token issuing + Rocket request guard for authenticated users.

use anyhow::Result;
use base64::Engine;
use rand::RngCore;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::token as auth_token;
use super::user::{self, User};
use super::{SESSION_COOKIE_NAME, SESSION_TTL_SECONDS};

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub async fn create(pool: &PgPool, token: &str, user_id: Uuid) -> Result<()> {
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(SESSION_TTL_SECONDS);
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(auth_token::token_hash(token))
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn lookup(pool: &PgPool, token: &str) -> Result<Option<Uuid>> {
    let row = sqlx::query_as::<_, (Uuid, OffsetDateTime)>(
        "SELECT user_id, expires_at FROM sessions WHERE token_hash = $1",
    )
    .bind(auth_token::token_hash(token))
    .fetch_optional(pool)
    .await?;
    let Some((user_id, expires_at)) = row else {
        return Ok(None);
    };
    if expires_at <= OffsetDateTime::now_utc() {
        return Ok(None);
    }
    Ok(Some(user_id))
}

pub async fn delete(pool: &PgPool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(auth_token::token_hash(token))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn purge_expired(pool: &PgPool) -> Result<u64> {
    let r = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(cookie) = req.cookies().get_private(SESSION_COOKIE_NAME) else {
            return Outcome::Forward(Status::Unauthorized);
        };
        let token = cookie.value().to_string();

        let pool = match req.rocket().state::<PgPool>() {
            Some(p) => p,
            None => {
                tracing::error!("User guard: PgPool not in Rocket state");
                return Outcome::Error((Status::InternalServerError, ()));
            }
        };

        let user_id = match lookup(pool, &token).await {
            Ok(Some(id)) => id,
            Ok(None) => return Outcome::Forward(Status::Unauthorized),
            Err(e) => {
                tracing::warn!("session lookup failed: {e}");
                return Outcome::Error((Status::InternalServerError, ()));
            }
        };

        match user::find_by_id(pool, user_id).await {
            Ok(Some(user)) => Outcome::Success(user),
            Ok(None) => Outcome::Forward(Status::Unauthorized),
            Err(e) => {
                tracing::warn!("user lookup failed: {e}");
                Outcome::Error((Status::InternalServerError, ()))
            }
        }
    }
}
