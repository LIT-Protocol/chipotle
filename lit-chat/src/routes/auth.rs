//! Identity routes (plans/tee-chat-app.md section 5).
//!
//! Magic-link login uses same-session code entry: the emailed short code is
//! typed into the originating tab. Identity travels in an enclave-signed
//! "pending" cookie bound to that tab; the DB row is a replay guard keyed by
//! a nonce hash and holds nothing identifying, so an operator with DB write
//! access cannot repoint a login at a victim account.

use crate::config::Config;
use crate::crypto::keys::KeySource;
use crate::crypto::{constant_time_eq, hmac_sha256};
use crate::identity::{self, UserKind};
use crate::mail::{code_email, Mailer};
use crate::rate_limit::StreamRateLimit;
use crate::routes::{err, ApiError, ChatUser, Csrf};
use crate::session::{self, SESSION_COOKIE};
use crate::store::{conversations, users};
use crate::{envelope, user_kek, Keyring};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use rand::Rng;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::serde::json::Json;
use rocket::{post, State};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

const PENDING_COOKIE: &str = "__Host-lit_chat_pending";
const MAGIC_LINK_TTL_SECS: i64 = 15 * 60;
/// Unambiguous alphabet (no 0/O, 1/I/l).
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LEN: usize = 8;

fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN)
        .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
        .collect()
}

fn build_session_cookie(token: String, ttl_secs: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(rocket::time::Duration::seconds(ttl_secs))
        .build()
}

// ---------------------------------------------------------------------------
// Pending magic-link token: payload "email|expires|nonce|code_hash" MAC'd
// with the enclave magic-link key. The signed token IS the identity binding.

struct PendingClaims {
    email: String,
    expires_unix: i64,
    nonce: String,
    code_hash: String,
}

fn issue_pending(mac_key: &[u8; 32], claims: &PendingClaims) -> String {
    let payload = format!(
        "{}|{}|{}|{}",
        claims.email, claims.expires_unix, claims.nonce, claims.code_hash
    );
    let sig = hmac_sha256(mac_key, payload.as_bytes());
    format!("{}.{}", B64.encode(payload.as_bytes()), B64.encode(sig))
}

fn verify_pending(mac_key: &[u8; 32], token: &str, now_unix: i64) -> Result<PendingClaims> {
    let (payload_b64, sig_b64) = token.split_once('.').context("pending: bad format")?;
    let payload_bytes = B64.decode(payload_b64).context("pending: bad payload")?;
    let provided = B64.decode(sig_b64).context("pending: bad sig")?;
    if !constant_time_eq(&provided, &hmac_sha256(mac_key, &payload_bytes)) {
        return Err(anyhow!("pending: signature mismatch"));
    }
    let payload = std::str::from_utf8(&payload_bytes).context("pending: non-utf8")?;
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() != 4 {
        return Err(anyhow!("pending: wrong field count"));
    }
    let expires_unix: i64 = parts[1].parse().context("pending: bad expiry")?;
    if expires_unix <= now_unix {
        return Err(anyhow!("pending: expired"));
    }
    Ok(PendingClaims {
        email: parts[0].to_string(),
        expires_unix,
        nonce: parts[2].to_string(),
        code_hash: parts[3].to_string(),
    })
}

fn hash_b64(data: &[u8]) -> String {
    use sha2::Digest;
    B64.encode(sha2::Sha256::digest(data))
}

// ---------------------------------------------------------------------------
// Routes

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

/// Mint an anonymous identity + session. Called by the frontend on first
/// visit (or after a lost cookie — old history is unreachable by design).
#[post("/api/session/anon")]
pub async fn anon_session(
    _rate: StreamRateLimit,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    cookies: &CookieJar<'_>,
) -> Result<Json<OkResponse>, ApiError> {
    let user_ref = identity::anon_user_ref();
    let hash = identity::user_ref_hash(&user_ref);
    users::upsert(pool, &hash, "anon").await.map_err(|e| {
        tracing::warn!("anon user upsert failed: {e}");
        err(Status::InternalServerError, "server_error")
    })?;
    let claims = session::SessionClaims {
        user_ref,
        kind: UserKind::Anon,
        expires_unix: session::now_unix() + session::ANON_SESSION_TTL_SECS,
        sid: session::generate_sid(),
    };
    let token = session::issue(&keyring.session_mac, &claims);
    cookies.add(build_session_cookie(token, session::ANON_SESSION_TTL_SECS));
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
pub struct RequestCodeBody {
    pub email: String,
}

/// Start magic-link login. Always returns ok (no email enumeration).
#[post("/api/auth/request", format = "json", data = "<body>")]
pub async fn request_code(
    _rate: StreamRateLimit,
    body: Json<RequestCodeBody>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    keyring: &State<Keyring>,
    mailer: &State<Option<Mailer>>,
    cookies: &CookieJar<'_>,
) -> Json<OkResponse> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') || email.len() > 254 {
        return Json(OkResponse { ok: true });
    }
    let code = generate_code();
    let nonce = hex::encode(rand::thread_rng().gen::<[u8; 16]>());
    let expires_unix = session::now_unix() + MAGIC_LINK_TTL_SECS;
    let claims = PendingClaims {
        email: email.clone(),
        expires_unix,
        nonce: nonce.clone(),
        code_hash: hash_b64(code.as_bytes()),
    };
    let token = issue_pending(&keyring.magic_link_mac, &claims);

    // Replay guard row: nonce hash only, nothing identifying.
    let expires_at = time::OffsetDateTime::from_unix_timestamp(expires_unix)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    if let Err(e) = users::store_magic_link(pool, &hash_b64(nonce.as_bytes()), expires_at).await {
        tracing::warn!("magic-link store failed: {e}");
        return Json(OkResponse { ok: true });
    }

    cookies.add(
        Cookie::build((PENDING_COOKIE, token))
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(rocket::time::Duration::seconds(MAGIC_LINK_TTL_SECS))
            .build(),
    );

    if cfg.dev_echo_codes {
        // Non-production builds only (config gates this): local dev without
        // Resend. The email address is still never logged.
        tracing::info!("dev sign-in code: {code}");
    } else if let Some(mailer) = mailer.inner() {
        let (subject, html, text) = code_email(&code);
        let mailer = mailer.clone();
        tokio::spawn(async move {
            if let Err(e) = mailer.send(&email, &subject, &html, &text).await {
                tracing::warn!("magic-link email send failed: {e}");
            }
        });
    } else {
        tracing::warn!("magic-link requested but no mailer configured");
    }
    Json(OkResponse { ok: true })
}

#[derive(Debug, Deserialize)]
pub struct VerifyCodeBody {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub ok: bool,
    pub kind: &'static str,
    pub migrated_conversations: u64,
}

/// Finish login in the originating tab. If the caller holds an anonymous
/// session, their history is migrated (DEKs rewrapped) to the account.
#[post("/api/auth/verify", format = "json", data = "<body>")]
pub async fn verify_code(
    _rate: StreamRateLimit,
    body: Json<VerifyCodeBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    key_source: &State<KeySource>,
    cookies: &CookieJar<'_>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let Some(pending_cookie) = cookies.get(PENDING_COOKIE) else {
        return Err(err(Status::BadRequest, "no_pending_login"));
    };
    let claims = verify_pending(
        &keyring.magic_link_mac,
        pending_cookie.value(),
        session::now_unix(),
    )
    .map_err(|_| err(Status::BadRequest, "invalid_or_expired"))?;

    let code = body.code.trim().to_uppercase();
    if !constant_time_eq(
        hash_b64(code.as_bytes()).as_bytes(),
        claims.code_hash.as_bytes(),
    ) {
        return Err(err(Status::BadRequest, "invalid_code"));
    }

    // Single-use consume of the replay guard.
    let consumed = users::consume_magic_link(pool, &hash_b64(claims.nonce.as_bytes()))
        .await
        .map_err(|e| {
            tracing::warn!("magic-link consume failed: {e}");
            err(Status::InternalServerError, "server_error")
        })?;
    if !consumed {
        return Err(err(Status::BadRequest, "invalid_or_expired"));
    }

    // The derivation IS the lookup (section 4.3).
    let account_ref = identity::account_user_ref(&keyring.user_id_namespace, &claims.email);
    let account_hash = identity::user_ref_hash(&account_ref);
    users::upsert(pool, &account_hash, "account")
        .await
        .map_err(|e| {
            tracing::warn!("account upsert failed: {e}");
            err(Status::InternalServerError, "server_error")
        })?;

    // Anon -> account migration if the caller holds a live anon session.
    let mut migrated = 0u64;
    if let Some(session_cookie) = cookies.get(SESSION_COOKIE) {
        if let Ok(old) = session::verify(
            &keyring.session_mac,
            session_cookie.value(),
            session::now_unix(),
        ) {
            if old.kind == UserKind::Anon {
                let anon_hash = identity::user_ref_hash(&old.user_ref);
                let anon_kek = user_kek(key_source, &old.user_ref).await.map_err(|e| {
                    tracing::warn!("anon KEK derive failed: {e}");
                    err(Status::InternalServerError, "server_error")
                })?;
                let account_kek = user_kek(key_source, &account_ref).await.map_err(|e| {
                    tracing::warn!("account KEK derive failed: {e}");
                    err(Status::InternalServerError, "server_error")
                })?;
                let account_hash_cl = account_hash.clone();
                let anon_hash_cl = anon_hash.clone();
                migrated = conversations::migrate_owner(
                    pool,
                    &anon_hash,
                    &account_hash,
                    move |conv_id, wrapped| {
                        let dek = envelope::unwrap_dek(&anon_kek, wrapped, &anon_hash_cl, conv_id)?;
                        envelope::wrap_dek(&account_kek, &dek, &account_hash_cl, conv_id)
                    },
                )
                .await
                .map_err(|e| {
                    tracing::warn!("anon->account migration failed: {e}");
                    err(Status::InternalServerError, "migration_failed")
                })?;
                // Retire the anon session token.
                let _ =
                    users::revoke_session(pool, &session::token_hash(session_cookie.value())).await;
            }
        }
    }

    let new_claims = session::SessionClaims {
        user_ref: account_ref,
        kind: UserKind::Account,
        expires_unix: session::now_unix() + session::ACCOUNT_SESSION_TTL_SECS,
        sid: session::generate_sid(),
    };
    let token = session::issue(&keyring.session_mac, &new_claims);
    cookies.add(build_session_cookie(
        token,
        session::ACCOUNT_SESSION_TTL_SECS,
    ));
    cookies.remove(Cookie::build((PENDING_COOKIE, "")).path("/").build());

    Ok(Json(VerifyResponse {
        ok: true,
        kind: "account",
        migrated_conversations: migrated,
    }))
}

/// Log out: revoke the current token and clear the cookie. For account users
/// this is real revocation; for anonymous users it permanently orphans the
/// history (disclosed in the UI).
#[post("/api/auth/logout")]
pub async fn logout(
    user: ChatUser,
    _csrf: Csrf,
    pool: &State<PgPool>,
    cookies: &CookieJar<'_>,
) -> Json<OkResponse> {
    if let Err(e) = users::revoke_session(pool, &user.token_hash).await {
        tracing::warn!("session revoke failed: {e}");
    }
    cookies.remove(Cookie::build((SESSION_COOKIE, "")).path("/").build());
    Json(OkResponse { ok: true })
}
