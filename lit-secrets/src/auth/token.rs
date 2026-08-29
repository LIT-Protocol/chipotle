//! HMAC-signed magic-link tokens.
//!
//! Format: `<base64url(email|expires_unix|nonce)>.<base64url(hmac_sha256(payload))>`
//!
//! The server signs every token it issues. Verification requires the secret
//! key, so an attacker cannot forge a token even if they know a user's
//! email. Issued token hashes are stored in `magic_links` and atomically
//! consumed so valid links are single-use.

use anyhow::{Context, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Build a signed magic-link token.
pub fn issue(signing_key: &[u8], email: &str, expires_unix: i64, nonce: &str) -> String {
    let payload = format!("{email}|{expires_unix}|{nonce}");
    let payload_b64 = B64.encode(payload.as_bytes());
    let sig = hmac_sha256(signing_key, payload.as_bytes());
    let sig_b64 = B64.encode(sig);
    format!("{payload_b64}.{sig_b64}")
}

/// Generate an unpredictable nonce to bind a signed token to one DB row.
pub fn generate_nonce() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    B64.encode(bytes)
}

/// Verified magic-link claims.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub email: String,
    pub expires_unix: i64,
    pub nonce: String,
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
    let mut parts = payload.split('|');
    let email = parts
        .next()
        .context("magic-link token: payload missing email")?;
    let expires_str = parts
        .next()
        .context("magic-link token: payload missing expiry")?;
    let nonce = parts
        .next()
        .context("magic-link token: payload missing nonce")?;
    if parts.next().is_some() {
        anyhow::bail!("magic-link token: payload has too many fields");
    }
    let expires_unix: i64 = expires_str
        .parse()
        .context("magic-link token: bad expiry timestamp")?;

    if expires_unix <= now_unix {
        anyhow::bail!("magic-link token: expired");
    }
    if nonce.is_empty() {
        anyhow::bail!("magic-link token: empty nonce");
    }

    Ok(VerifiedClaims {
        email: email.to_string(),
        expires_unix,
        nonce: nonce.to_string(),
    })
}

/// Stable, non-reversible hash for DB storage/lookups of bearer and magic-link tokens.
pub fn token_hash(token: &str) -> String {
    use sha2::Digest;

    let digest = Sha256::digest(token.as_bytes());
    B64.encode(digest)
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
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000, "nonce-1");
        let claims = verify(KEY, &token, 1_799_000_000).unwrap();
        assert_eq!(claims.email, "chris@litprotocol.com");
        assert_eq!(claims.expires_unix, 1_800_000_000);
        assert_eq!(claims.nonce, "nonce-1");
    }

    #[test]
    fn rejects_expired_token() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000, "nonce-1");
        let err = verify(KEY, &token, 1_900_000_000).unwrap_err();
        assert!(err.to_string().contains("expired"), "{err}");
    }

    #[test]
    fn rejects_tampered_payload() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000, "nonce-1");
        // Swap the payload half for a different one without re-signing.
        let (_, sig) = token.split_once('.').unwrap();
        let fake_payload = B64.encode(b"attacker@evil.com|1800000000|nonce-1");
        let tampered = format!("{fake_payload}.{sig}");
        let err = verify(KEY, &tampered, 1_700_000_000).unwrap_err();
        assert!(err.to_string().contains("signature mismatch"), "{err}");
    }

    #[test]
    fn rejects_wrong_key() {
        let token = issue(KEY, "chris@litprotocol.com", 1_800_000_000, "nonce-1");
        let err = verify(b"different-key-32-bytes-xxxxxxxx", &token, 1_700_000_000).unwrap_err();
        assert!(err.to_string().contains("signature mismatch"), "{err}");
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(verify(KEY, "nopedotseparator", 1_000).is_err());
        assert!(verify(KEY, "!!!.!!!", 1_000).is_err());
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn token_hash_is_deterministic_and_non_revealing() {
        let raw = "raw-secret-token";
        let hash = token_hash(raw);
        assert_eq!(hash, token_hash(raw));
        assert_ne!(hash, raw);
        assert!(!hash.contains(raw));
    }
}
