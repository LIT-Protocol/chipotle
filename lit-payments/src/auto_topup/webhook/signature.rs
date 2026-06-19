//! Stripe-Signature verification.
//!
//! Stripe sends each webhook with a header of the form
//! `t={unix_ts},v1={hex_hmac_sha256}`. We:
//!
//! 1. Parse the header. Reject if it doesn't match the expected schema.
//! 2. Reject if `|now - t| > 300` (5-minute timestamp skew tolerance —
//!    Stripe's recommended floor and the same window CPL-329's invoice
//!    webhook handler uses).
//! 3. Compute `HMAC-SHA256(secret, "{t}.{raw_body}")` and constant-time
//!    compare against `v1`. Constant-time matters: a normal `==` short-
//!    circuits on the first mismatched byte and an attacker can in theory
//!    learn the secret byte-by-byte from response timing.
//!
//! Stripe occasionally sends multiple `v1=` entries during webhook secret
//! rotation. Any one match passes verification.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Maximum acceptable skew between the timestamp Stripe signed and our
/// system clock. 300 seconds matches Stripe's documented recommendation.
pub const TIMESTAMP_SKEW_SECONDS: i64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("missing or empty Stripe-Signature header")]
    Missing,
    #[error("malformed Stripe-Signature header: {0}")]
    Malformed(String),
    #[error("timestamp skew exceeds tolerance: |now - t| = {0}s > {TIMESTAMP_SKEW_SECONDS}s")]
    TimestampSkew(i64),
    #[error("no v1 signature matched the expected HMAC")]
    NoMatch,
}

/// Verify the `Stripe-Signature` header value against `raw_body` using the
/// shared webhook secret. `now_unix` is parameterized so tests can pin time;
/// production callers pass `std::time::SystemTime::now()`-derived seconds.
pub fn verify(
    header_value: &str,
    raw_body: &[u8],
    secret: &str,
    now_unix: i64,
) -> Result<(), SignatureError> {
    if header_value.trim().is_empty() {
        return Err(SignatureError::Missing);
    }

    let mut timestamp: Option<i64> = None;
    let mut v1_signatures: Vec<&str> = Vec::new();
    for part in header_value.split(',') {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| SignatureError::Malformed(format!("missing '=' in {part:?}")))?;
        match k.trim() {
            "t" => {
                timestamp = Some(
                    v.trim()
                        .parse::<i64>()
                        .map_err(|e| SignatureError::Malformed(format!("t not int: {e}")))?,
                );
            }
            "v1" => v1_signatures.push(v.trim()),
            _ => {
                // Stripe documents v0 (deprecated) and forward-compat
                // entries; ignore unknown schemes.
            }
        }
    }

    let t = timestamp.ok_or_else(|| SignatureError::Malformed("no t=...".into()))?;
    let skew = (now_unix - t).abs();
    if skew > TIMESTAMP_SKEW_SECONDS {
        return Err(SignatureError::TimestampSkew(skew));
    }
    if v1_signatures.is_empty() {
        return Err(SignatureError::Malformed("no v1=...".into()));
    }

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    // Stripe signs the exact string `{timestamp}.{raw_body}`.
    mac.update(t.to_string().as_bytes());
    mac.update(b".");
    mac.update(raw_body);
    let expected = mac.finalize().into_bytes();

    for sig_hex in v1_signatures {
        if let Ok(presented) = hex::decode(sig_hex)
            && presented.len() == expected.len()
            && bool::from(presented.ct_eq(&expected))
        {
            return Ok(());
        }
    }
    Err(SignatureError::NoMatch)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test_secret_value_for_unit_tests";

    fn sign(timestamp: i64, body: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(SECRET.as_bytes()).expect("HMAC accepts any key length");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        let hex_sig = hex::encode(mac.finalize().into_bytes());
        format!("t={timestamp},v1={hex_sig}")
    }

    #[test]
    fn accepts_valid_signature_within_window() {
        let now = 1_780_000_000;
        let body = br#"{"id":"evt_xxx","type":"customer.updated"}"#;
        let header = sign(now, body);
        assert!(verify(&header, body, SECRET, now).is_ok());
    }

    #[test]
    fn accepts_within_300s_skew() {
        let now = 1_780_000_000;
        let signed_at = now - 299;
        let body = b"body";
        let header = sign(signed_at, body);
        assert!(verify(&header, body, SECRET, now).is_ok());
    }

    #[test]
    fn rejects_skew_beyond_tolerance() {
        let now = 1_780_000_000;
        let signed_at = now - 301;
        let body = b"body";
        let header = sign(signed_at, body);
        match verify(&header, body, SECRET, now) {
            Err(SignatureError::TimestampSkew(s)) => assert!(s > TIMESTAMP_SKEW_SECONDS),
            other => panic!("expected TimestampSkew, got {other:?}"),
        }
    }

    #[test]
    fn rejects_future_skew_beyond_tolerance() {
        let now = 1_780_000_000;
        let signed_at = now + 301;
        let body = b"body";
        let header = sign(signed_at, body);
        match verify(&header, body, SECRET, now) {
            Err(SignatureError::TimestampSkew(_)) => {}
            other => panic!("expected TimestampSkew, got {other:?}"),
        }
    }

    #[test]
    fn rejects_body_tampering() {
        let now = 1_780_000_000;
        let body = b"original-body";
        let header = sign(now, body);
        let tampered = b"tampered-body";
        assert!(matches!(
            verify(&header, tampered, SECRET, now),
            Err(SignatureError::NoMatch)
        ));
    }

    #[test]
    fn rejects_wrong_secret() {
        let now = 1_780_000_000;
        let body = b"body";
        let header = sign(now, body);
        assert!(matches!(
            verify(&header, body, "different_secret", now),
            Err(SignatureError::NoMatch)
        ));
    }

    #[test]
    fn rejects_missing_v1() {
        let header = "t=1780000000";
        assert!(matches!(
            verify(header, b"body", SECRET, 1_780_000_000),
            Err(SignatureError::Malformed(_))
        ));
    }

    #[test]
    fn rejects_missing_timestamp() {
        let header = "v1=abcdef";
        assert!(matches!(
            verify(header, b"body", SECRET, 1_780_000_000),
            Err(SignatureError::Malformed(_))
        ));
    }

    #[test]
    fn rejects_empty_header() {
        assert!(matches!(
            verify("", b"body", SECRET, 1_780_000_000),
            Err(SignatureError::Missing)
        ));
    }

    #[test]
    fn accepts_multiple_v1_entries_during_rotation() {
        // Stripe sends both old + new v1 signatures during webhook secret
        // rotation. Any single match must pass.
        let now = 1_780_000_000;
        let body = b"body";
        let valid = sign(now, body);
        // valid is "t=...,v1=hex" — append a junk v1.
        let with_extra = format!("{valid},v1=deadbeef00");
        assert!(verify(&with_extra, body, SECRET, now).is_ok());
    }
}
