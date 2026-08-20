//! HMAC-signed magic-link tokens.
//!
//! Format: `<base64url(email|expires_unix)>.<base64url(hmac_sha256(payload))>`
//!
//! The server signs every token it issues. Verification requires the secret
//! key, so an attacker cannot forge a token even if they know an operator's
//! email. As a second line of defense, the verify path also checks the
//! decoded email is present in the `operators` table — a leaked key would
//! still only let an attacker log in as accounts that already exist.

use anyhow::{Context, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Build a signed magic-link token.
pub fn issue(signing_key: &[u8], email: &str, expires_unix: i64) -> String {
    let payload = format!("{email}|{expires_unix}");
    let payload_b64 = B64.encode(payload.as_bytes());
    let sig = hmac_sha256(signing_key, payload.as_bytes());
    let sig_b64 = B64.encode(sig);
    format!("{payload_b64}.{sig_b64}")
}

/// SHA-256 hex digest of a raw magic-link token. Used as the single-use dedup
/// key in `used_magic_links` so the raw token is never persisted (CPL-379 L8).
pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Verified magic-link claims.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub email: String,
    pub expires_unix: i64,
}

/// Parse and verify a magic-link token.
///
/// Returns `Err` if the token is malformed, the signature doesn't match, or
/// `expires_unix <= now_unix`. The error message is intentionally generic
/// for the verify-route response, but the inner `anyhow::Error` chain is
/// useful for server-side logs.
pub fn verify(signing_key: &[u8], token: &str, now_unix: i64) -> Result<VerifiedClaims> {
    let (payload_b64, sig_b64) = token
        .split_once('.')
        .context("magic-link token: missing separator")?;
    let payload_bytes = B64
        .decode(payload_b64)
        .context("magic-link token: bad payload base64")?;
    let provided_sig = B64
        .decode(sig_b64)
        .context("magic-link token: bad signature base64")?;

    let expected_sig = hmac_sha256(signing_key, &payload_bytes);
    if !constant_time_eq(&provided_sig, &expected_sig) {
        anyhow::bail!("magic-link token: signature mismatch");
    }

    let payload =
        std::str::from_utf8(&payload_bytes).context("magic-link token: non-utf8 payload")?;
    let (email, expires_str) = payload
        .split_once('|')
        .context("magic-link token: payload missing separator")?;
    let expires_unix: i64 = expires_str
        .parse()
        .context("magic-link token: bad expiry timestamp")?;

    if expires_unix <= now_unix {
        anyhow::bail!("magic-link token: expired");
    }

    Ok(VerifiedClaims {
        email: email.to_string(),
        expires_unix,
    })
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(msg);
    mac.finalize().into_bytes().into()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"a-secret-signing-key-at-least-32-bytes-long";

    #[test]
    fn roundtrip_valid_token() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000);
        let claims = verify(KEY, &token, 1_799_000_000).unwrap();
        assert_eq!(claims.email, "chris@litprotocol.com");
        assert_eq!(claims.expires_unix, 1_800_000_000);
    }

    #[test]
    fn rejects_expired_token() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000);
        let err = verify(KEY, &token, 1_900_000_000).unwrap_err();
        assert!(err.to_string().contains("expired"), "{err}");
    }

    #[test]
    fn rejects_tampered_payload() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000);
        // Swap the payload half for a different one without re-signing.
        let (_, sig) = token.split_once('.').unwrap();
        let fake_payload = B64.encode(b"attacker@evil.com|1800000000");
        let tampered = format!("{fake_payload}.{sig}");
        let err = verify(KEY, &tampered, 1_700_000_000).unwrap_err();
        assert!(err.to_string().contains("signature mismatch"), "{err}");
    }

    #[test]
    fn rejects_wrong_key() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000);
        let err = verify(b"different-key-32-bytes-xxxxxxxx", &token, 1_700_000_000).unwrap_err();
        assert!(err.to_string().contains("signature mismatch"), "{err}");
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(verify(KEY, "nopedotseparator", 1_000).is_err());
        assert!(verify(KEY, "!!!.!!!", 1_000).is_err());
    }

    #[test]
    fn token_hash_is_deterministic_and_distinct() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000);
        // Same token → same hash (so a replay maps to the same PK row).
        assert_eq!(token_hash(&token), token_hash(&token));
        // Different token → different hash.
        let other = issue(KEY, "chris@litprotocol.com", 1_800_000_001);
        assert_ne!(token_hash(&token), token_hash(&other));
        // SHA-256 hex is 64 chars and never leaks the raw token.
        let h = token_hash(&token);
        assert_eq!(h.len(), 64);
        assert!(!h.contains(&token));
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
