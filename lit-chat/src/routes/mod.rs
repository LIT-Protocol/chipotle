//! Consumer app routes and request guards.

pub mod auth;
pub mod chat;

use crate::identity::UserKind;
use crate::session::{self, SESSION_COOKIE};
use crate::store::provider_keys;
use crate::Keyring;
use moka::future::Cache;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use serde::Serialize;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// API error convention (lit-triggers shape): stable machine-readable slugs;
// details only in logs.

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: &'static str,
}

pub type ApiError = rocket::response::status::Custom<Json<ErrorResponse>>;

pub fn err(status: Status, error: &'static str) -> ApiError {
    rocket::response::status::Custom(status, Json(ErrorResponse { error }))
}

// ---------------------------------------------------------------------------
// Session guard: verifies the TEE-signed cookie, then checks the DB
// revocation list. No DB row is ever authoritative for identity.

pub struct ChatUser {
    pub user_ref: String,
    pub user_ref_hash: String,
    pub kind: UserKind,
    pub sid: String,
    pub token_hash: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ChatUser {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(keyring) = req.rocket().state::<Keyring>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        let Some(pool) = req.rocket().state::<PgPool>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        let Some(cookie) = req.cookies().get(SESSION_COOKIE) else {
            return Outcome::Error((Status::Unauthorized, ()));
        };
        let token = cookie.value();
        let claims = match session::verify(&keyring.session_mac, token, session::now_unix()) {
            Ok(c) => c,
            Err(_) => return Outcome::Error((Status::Unauthorized, ())),
        };
        let token_hash = session::token_hash(token);
        match crate::store::users::session_revoked(pool, &token_hash).await {
            Ok(false) => {}
            Ok(true) => return Outcome::Error((Status::Unauthorized, ())),
            Err(e) => {
                tracing::warn!("revocation check failed: {e}");
                return Outcome::Error((Status::InternalServerError, ()));
            }
        }
        Outcome::Success(ChatUser {
            user_ref_hash: crate::identity::user_ref_hash(&claims.user_ref),
            user_ref: claims.user_ref,
            kind: claims.kind,
            sid: claims.sid,
            token_hash,
        })
    }
}

// ---------------------------------------------------------------------------
// CSRF guard for state-changing routes: the `X-CSRF-Token` header must match
// the session-bound token (cookie is SameSite=Lax as the first layer).

pub struct Csrf;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Csrf {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(keyring) = req.rocket().state::<Keyring>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        let Some(cookie) = req.cookies().get(SESSION_COOKIE) else {
            return Outcome::Error((Status::Forbidden, ()));
        };
        let claims =
            match session::verify(&keyring.session_mac, cookie.value(), session::now_unix()) {
                Ok(c) => c,
                Err(_) => return Outcome::Error((Status::Forbidden, ())),
            };
        let Some(header) = req.headers().get_one("X-CSRF-Token") else {
            return Outcome::Error((Status::Forbidden, ()));
        };
        let expected = session::csrf_for(&keyring.session_mac, &claims.sid);
        if crate::crypto::constant_time_eq(header.as_bytes(), expected.as_bytes()) {
            Outcome::Success(Csrf)
        } else {
            Outcome::Error((Status::Forbidden, ()))
        }
    }
}

// ---------------------------------------------------------------------------
// Short-TTL cache over the active runtime key (version-keyed shape from the
// PRD, simplified to a time bound): key swaps in the admin console take
// effect here within seconds, no restart.

#[derive(Clone)]
pub struct RuntimeKeyCache {
    cache: Cache<u8, (String, Uuid)>,
}

impl Default for RuntimeKeyCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeKeyCache {
    pub fn new() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(10))
                .build(),
        }
    }

    pub async fn get(
        &self,
        pool: &PgPool,
        provider_kek: &[u8; 32],
    ) -> anyhow::Result<Option<(String, Uuid)>> {
        if let Some(hit) = self.cache.get(&0).await {
            return Ok(Some(hit));
        }
        let Some(row) = provider_keys::active_runtime(pool, "openrouter").await? else {
            return Ok(None);
        };
        let key = provider_keys::decrypt_key(provider_kek, &row)?;
        let entry = (key, row.id);
        self.cache.insert(0, entry.clone()).await;
        Ok(Some(entry))
    }
}
