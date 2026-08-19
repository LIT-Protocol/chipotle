//! Admin plane (plans/tee-chat-app.md section 4.4).
//!
//! Isolation is load-bearing: this module has zero user-data routes, uses
//! different dstack purpose paths than the chat KEK derivations, and the
//! deployment connects to Postgres as a role with no grants on chat tables.
//! A fully compromised admin plane can spend money and break inference; it
//! cannot read chats.

pub mod routes;

use crate::crypto::{constant_time_eq, hmac_sha256};
use crate::session::ADMIN_SESSION_COOKIE;
use crate::store::admin as admin_store;
use crate::Keyring;
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use moka::future::Cache;
use rocket::http::{Cookie, SameSite, Status};
use rocket::request::{FromRequest, Outcome, Request};
use sqlx::PgPool;
use std::time::Duration;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Short admin sessions: 15 min sliding, 8 h absolute (section 4.4).
pub const ADMIN_IDLE_TTL_SECS: i64 = 15 * 60;
pub const ADMIN_ABSOLUTE_TTL_SECS: i64 = 8 * 3600;
/// Pre-second-factor window.
pub const PRE2FA_TTL_SECS: i64 = 10 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminStage {
    /// Magic link verified, passkey not yet presented/registered.
    Pre2fa,
    /// Passkey verified: full admin session.
    Full,
}

impl AdminStage {
    fn as_str(&self) -> &'static str {
        match self {
            AdminStage::Pre2fa => "pre2fa",
            AdminStage::Full => "full",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "pre2fa" => Some(AdminStage::Pre2fa),
            "full" => Some(AdminStage::Full),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdminClaims {
    pub user_ref: String,
    pub stage: AdminStage,
    pub issued_unix: i64,
    pub expires_unix: i64,
    pub sid: String,
}

pub fn issue_admin(mac_key: &[u8; 32], claims: &AdminClaims) -> String {
    let payload = format!(
        "{}|{}|{}|{}|{}",
        claims.user_ref,
        claims.stage.as_str(),
        claims.issued_unix,
        claims.expires_unix,
        claims.sid
    );
    let sig = hmac_sha256(mac_key, payload.as_bytes());
    format!("{}.{}", B64.encode(payload.as_bytes()), B64.encode(sig))
}

pub fn verify_admin(mac_key: &[u8; 32], token: &str, now_unix: i64) -> Result<AdminClaims> {
    let (payload_b64, sig_b64) = token.split_once('.').context("admin session: bad format")?;
    let payload_bytes = B64
        .decode(payload_b64)
        .context("admin session: bad payload")?;
    let provided = B64.decode(sig_b64).context("admin session: bad sig")?;
    if !constant_time_eq(&provided, &hmac_sha256(mac_key, &payload_bytes)) {
        return Err(anyhow!("admin session: signature mismatch"));
    }
    let payload = std::str::from_utf8(&payload_bytes).context("admin session: non-utf8")?;
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() != 5 {
        return Err(anyhow!("admin session: wrong field count"));
    }
    let stage = AdminStage::parse(parts[1]).context("admin session: bad stage")?;
    let issued_unix: i64 = parts[2].parse().context("admin session: bad issued")?;
    let expires_unix: i64 = parts[3].parse().context("admin session: bad expiry")?;
    if expires_unix <= now_unix {
        return Err(anyhow!("admin session: expired"));
    }
    if now_unix - issued_unix > ADMIN_ABSOLUTE_TTL_SECS {
        return Err(anyhow!("admin session: absolute lifetime exceeded"));
    }
    Ok(AdminClaims {
        user_ref: parts[0].to_string(),
        stage,
        issued_unix,
        expires_unix,
        sid: parts[4].to_string(),
    })
}

pub fn admin_cookie(token: String, ttl_secs: i64) -> Cookie<'static> {
    Cookie::build((ADMIN_SESSION_COOKIE, token))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(rocket::time::Duration::seconds(ttl_secs))
        .build()
}

pub fn admin_csrf(mac_key: &[u8; 32], sid: &str) -> String {
    B64.encode(hmac_sha256(
        mac_key,
        format!("chat.admin-csrf.v1|{sid}").as_bytes(),
    ))
}

// ---------------------------------------------------------------------------
// Guards

/// Full admin: valid TEE-signed session at stage `full`, revocation-checked,
/// AND a MAC-valid roster row (checked on every request — a revoked or
/// tampered roster row cuts access immediately).
pub struct AdminUser {
    pub user_ref: String,
    pub user_ref_hash: String,
    pub sid: String,
    pub token_hash: String,
}

/// Pre-2FA admin: allowed to touch only the WebAuthn endpoints. Carries the
/// session stage so the register handlers can enforce first-enrollment-only
/// at Pre2fa (a Pre2fa caller may enroll a passkey ONLY when the account has
/// none; adding another requires a Full, passkey-proven session).
pub struct PendingAdmin {
    pub user_ref: String,
    pub user_ref_hash: String,
    pub sid: String,
    pub stage: AdminStage,
}

async fn extract_admin(
    req: &Request<'_>,
    required_stage: Option<AdminStage>,
) -> Result<(AdminClaims, String), Status> {
    let keyring = req
        .rocket()
        .state::<Keyring>()
        .ok_or(Status::InternalServerError)?;
    let pool = req
        .rocket()
        .state::<PgPool>()
        .ok_or(Status::InternalServerError)?;
    let cookie = req
        .cookies()
        .get(ADMIN_SESSION_COOKIE)
        .ok_or(Status::Unauthorized)?;
    let token = cookie.value();
    let now = crate::session::now_unix();
    let claims =
        verify_admin(&keyring.admin_session_mac, token, now).map_err(|_| Status::Unauthorized)?;
    if let Some(stage) = required_stage {
        if claims.stage != stage {
            return Err(Status::Unauthorized);
        }
    }
    let token_hash = crate::session::token_hash(token);
    match crate::store::users::session_revoked(pool, &token_hash).await {
        Ok(false) => {}
        _ => return Err(Status::Unauthorized),
    }
    // Roster re-check on every request.
    let hash = crate::identity::user_ref_hash(&claims.user_ref);
    match admin_store::is_admin(pool, &keyring.admin_roster_mac, &hash).await {
        Ok(true) => {}
        _ => return Err(Status::Forbidden),
    }
    Ok((claims, token_hash))
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AdminUser {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match extract_admin(req, Some(AdminStage::Full)).await {
            Ok((claims, token_hash)) => {
                // Sliding refresh: extend the 15-minute idle window, bounded
                // by the 8-hour absolute lifetime carried in issued_unix.
                let keyring = req.rocket().state::<Keyring>().expect("keyring");
                let now = crate::session::now_unix();
                let refreshed = AdminClaims {
                    expires_unix: (now + ADMIN_IDLE_TTL_SECS)
                        .min(claims.issued_unix + ADMIN_ABSOLUTE_TTL_SECS),
                    ..claims.clone()
                };
                let token = issue_admin(&keyring.admin_session_mac, &refreshed);
                req.cookies().add(admin_cookie(token, ADMIN_IDLE_TTL_SECS));
                Outcome::Success(AdminUser {
                    user_ref_hash: crate::identity::user_ref_hash(&claims.user_ref),
                    user_ref: claims.user_ref,
                    sid: claims.sid,
                    token_hash,
                })
            }
            Err(status) => Outcome::Error((status, ())),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PendingAdmin {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // Accept either stage: a full admin may register additional passkeys
        // (the register handlers enforce the first-enrollment-only rule using
        // the stage carried here).
        match extract_admin(req, None).await {
            Ok((claims, _)) => Outcome::Success(PendingAdmin {
                user_ref_hash: crate::identity::user_ref_hash(&claims.user_ref),
                user_ref: claims.user_ref,
                sid: claims.sid,
                stage: claims.stage,
            }),
            Err(status) => Outcome::Error((status, ())),
        }
    }
}

/// CSRF guard for admin state-changing routes.
pub struct AdminCsrf;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AdminCsrf {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(keyring) = req.rocket().state::<Keyring>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        let Some(cookie) = req.cookies().get(ADMIN_SESSION_COOKIE) else {
            return Outcome::Error((Status::Forbidden, ()));
        };
        let claims = match verify_admin(
            &keyring.admin_session_mac,
            cookie.value(),
            crate::session::now_unix(),
        ) {
            Ok(c) => c,
            Err(_) => return Outcome::Error((Status::Forbidden, ())),
        };
        let Some(header) = req.headers().get_one("X-CSRF-Token") else {
            return Outcome::Error((Status::Forbidden, ()));
        };
        let expected = admin_csrf(&keyring.admin_session_mac, &claims.sid);
        if constant_time_eq(header.as_bytes(), expected.as_bytes()) {
            Outcome::Success(AdminCsrf)
        } else {
            Outcome::Error((Status::Forbidden, ()))
        }
    }
}

// ---------------------------------------------------------------------------
// WebAuthn state: in-flight registration/authentication challenges, held
// in-enclave only (serialized, short TTL, keyed by session id).

#[derive(Clone)]
pub struct WebauthnState {
    pub webauthn: std::sync::Arc<webauthn_rs::Webauthn>,
    reg: Cache<String, String>,
    auth: Cache<String, String>,
}

impl WebauthnState {
    pub fn new(rp_id: &str, origin: &str) -> Result<Self> {
        let origin_url = url::Url::parse(origin).context("parsing LIT_CHAT_ADMIN_ORIGIN")?;
        let webauthn = webauthn_rs::WebauthnBuilder::new(rp_id, &origin_url)
            .context("building webauthn")?
            .rp_name("Lit Chat Admin")
            .build()
            .context("building webauthn")?;
        let ttl = Duration::from_secs(300);
        Ok(Self {
            webauthn: std::sync::Arc::new(webauthn),
            reg: Cache::builder()
                .max_capacity(1000)
                .time_to_live(ttl)
                .build(),
            auth: Cache::builder()
                .max_capacity(1000)
                .time_to_live(ttl)
                .build(),
        })
    }

    pub async fn put_reg(&self, sid: &str, state_json: String) {
        self.reg.insert(sid.to_string(), state_json).await;
    }

    pub async fn take_reg(&self, sid: &str) -> Option<String> {
        let v = self.reg.get(sid).await;
        self.reg.invalidate(sid).await;
        v
    }

    pub async fn put_auth(&self, sid: &str, state_json: String) {
        self.auth.insert(sid.to_string(), state_json).await;
    }

    pub async fn take_auth(&self, sid: &str) -> Option<String> {
        let v = self.auth.get(sid).await;
        self.auth.invalidate(sid).await;
        v
    }
}

/// Boot-time roster bootstrap: the bootstrap admin set ships in
/// encrypted_env (sealed, part of the governed deploy); rows are MAC'd like
/// any runtime grant.
pub async fn bootstrap_admins(pool: &PgPool, keyring: &Keyring, emails: &[String]) -> Result<()> {
    for email in emails {
        let user_ref = crate::identity::account_user_ref(&keyring.user_id_namespace, email);
        let hash = crate::identity::user_ref_hash(&user_ref);
        admin_store::grant(pool, &keyring.admin_roster_mac, &hash, "bootstrap").await?;
        tracing::info!(user_ref_hash = %hash, "bootstrap admin ensured");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_session_roundtrip_and_stage() {
        let key = [9u8; 32];
        let now = crate::session::now_unix();
        let claims = AdminClaims {
            user_ref: "acct:aa".into(),
            stage: AdminStage::Pre2fa,
            issued_unix: now,
            expires_unix: now + 600,
            sid: "s1".into(),
        };
        let tok = issue_admin(&key, &claims);
        let v = verify_admin(&key, &tok, now).unwrap();
        assert_eq!(v.stage, AdminStage::Pre2fa);
        assert_eq!(v.user_ref, "acct:aa");
    }

    #[test]
    fn absolute_lifetime_enforced() {
        let key = [9u8; 32];
        let now = crate::session::now_unix();
        let claims = AdminClaims {
            user_ref: "acct:aa".into(),
            stage: AdminStage::Full,
            issued_unix: now - ADMIN_ABSOLUTE_TTL_SECS - 1,
            // Forged/refreshed expiry can't outlive the absolute bound.
            expires_unix: now + 600,
            sid: "s1".into(),
        };
        let tok = issue_admin(&key, &claims);
        assert!(verify_admin(&key, &tok, now).is_err());
    }

    #[test]
    fn consumer_and_admin_macs_are_not_interchangeable() {
        // A consumer session signed with the consumer MAC key must not
        // verify as an admin session even if key material leaked across —
        // different keys by derivation; here we just assert format rejects.
        let key = [9u8; 32];
        let consumer = crate::session::issue(
            &key,
            &crate::session::SessionClaims {
                user_ref: "acct:aa".into(),
                kind: crate::identity::UserKind::Account,
                expires_unix: crate::session::now_unix() + 600,
                sid: "s1".into(),
            },
        );
        assert!(verify_admin(&key, &consumer, crate::session::now_unix()).is_err());
    }
}
