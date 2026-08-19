//! TEE-signed sessions (plans/tee-chat-app.md section 5.1).
//!
//! Tokens are minted and MAC'd inside the enclave with a dstack-derived
//! service secret. The database holds at most a revocation list — never
//! authoritative session rows — so a DB operator cannot mint a valid session
//! for any user without the enclave.
//!
//! Format: `base64url(user_ref|kind|expires_unix|sid).base64url(hmac_sha256)`.

use crate::crypto::{constant_time_eq, hmac_sha256};
use crate::identity::UserKind;
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use rand::RngCore;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub const SESSION_COOKIE: &str = "__Host-lit_chat_session";
pub const ADMIN_SESSION_COOKIE: &str = "__Host-lit_chat_admin_session";
/// Anonymous sessions: long-lived by design — the cookie is the capability.
pub const ANON_SESSION_TTL_SECS: i64 = 180 * 24 * 3600;
pub const ACCOUNT_SESSION_TTL_SECS: i64 = 30 * 24 * 3600;

#[derive(Debug, Clone)]
pub struct SessionClaims {
    pub user_ref: String,
    pub kind: UserKind,
    pub expires_unix: i64,
    /// Random per-session id; input to the CSRF token derivation.
    pub sid: String,
}

pub fn generate_sid() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn issue(mac_key: &[u8; 32], claims: &SessionClaims) -> String {
    let payload = format!(
        "{}|{}|{}|{}",
        claims.user_ref,
        claims.kind.as_str(),
        claims.expires_unix,
        claims.sid
    );
    let sig = hmac_sha256(mac_key, payload.as_bytes());
    format!("{}.{}", B64.encode(payload.as_bytes()), B64.encode(sig))
}

pub fn verify(mac_key: &[u8; 32], token: &str, now_unix: i64) -> Result<SessionClaims> {
    let (payload_b64, sig_b64) = token
        .split_once('.')
        .context("session: missing separator")?;
    let payload_bytes = B64.decode(payload_b64).context("session: bad payload")?;
    let provided = B64.decode(sig_b64).context("session: bad signature")?;
    let expected = hmac_sha256(mac_key, &payload_bytes);
    if !constant_time_eq(&provided, &expected) {
        return Err(anyhow!("session: signature mismatch"));
    }
    let payload = std::str::from_utf8(&payload_bytes).context("session: non-utf8")?;
    let mut parts = payload.split('|');
    let user_ref = parts.next().context("session: missing ref")?;
    let kind = parts
        .next()
        .and_then(UserKind::parse)
        .context("session: bad kind")?;
    let expires_unix: i64 = parts
        .next()
        .context("session: missing expiry")?
        .parse()
        .context("session: bad expiry")?;
    let sid = parts.next().context("session: missing sid")?;
    if parts.next().is_some() {
        return Err(anyhow!("session: too many fields"));
    }
    if expires_unix <= now_unix {
        return Err(anyhow!("session: expired"));
    }
    Ok(SessionClaims {
        user_ref: user_ref.to_string(),
        kind,
        expires_unix,
        sid: sid.to_string(),
    })
}

/// Stable, non-reversible hash for the revocation list.
pub fn token_hash(token: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(token.as_bytes());
    B64.encode(digest)
}

/// CSRF token bound to the session: recomputable server-side, unforgeable
/// without the session MAC key. Sent by the frontend as `X-CSRF-Token` on
/// every state-changing request (cookie is SameSite=Lax as well).
pub fn csrf_for(mac_key: &[u8; 32], sid: &str) -> String {
    let mac = hmac_sha256(mac_key, format!("chat.csrf.v1|{sid}").as_bytes());
    B64.encode(mac)
}

pub fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> SessionClaims {
        SessionClaims {
            user_ref: "anon:00ff".into(),
            kind: UserKind::Anon,
            expires_unix: now_unix() + 3600,
            sid: generate_sid(),
        }
    }

    #[test]
    fn roundtrip() {
        let key = [5u8; 32];
        let c = claims();
        let tok = issue(&key, &c);
        let v = verify(&key, &tok, now_unix()).unwrap();
        assert_eq!(v.user_ref, c.user_ref);
        assert_eq!(v.sid, c.sid);
        assert_eq!(v.kind, UserKind::Anon);
    }

    #[test]
    fn wrong_key_rejected() {
        let tok = issue(&[5u8; 32], &claims());
        assert!(verify(&[6u8; 32], &tok, now_unix()).is_err());
    }

    #[test]
    fn expired_rejected() {
        let key = [5u8; 32];
        let mut c = claims();
        c.expires_unix = now_unix() - 1;
        let tok = issue(&key, &c);
        assert!(verify(&key, &tok, now_unix()).is_err());
    }

    #[test]
    fn tampered_payload_rejected() {
        let key = [5u8; 32];
        let tok = issue(&key, &claims());
        let (payload, sig) = tok.split_once('.').unwrap();
        // Re-encode a different payload with the original signature.
        let forged = format!(
            "{}.{sig}",
            B64.encode(format!("acct:beef|account|{}|{}", now_unix() + 3600, "00"))
        );
        assert!(verify(&key, &forged, now_unix()).is_err());
        let _ = payload;
    }

    #[test]
    fn csrf_is_deterministic_per_sid() {
        let key = [5u8; 32];
        assert_eq!(csrf_for(&key, "abc"), csrf_for(&key, "abc"));
        assert_ne!(csrf_for(&key, "abc"), csrf_for(&key, "abd"));
    }
}
