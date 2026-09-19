//! Per-API-key spending rules + rolling usage.
//!
//! Storage and HTTP surface for the Lambda-parity blast-radius controls
//! (rolling spend cap, rate/concurrency limits, origin allowlist) that the
//! gateway enforces on frontend-callable usage keys. See
//! `plans/chipotle-lambda-parity.md`.
//!
//! - Operator (cookie-authed) routes under `/api/spending-rules` let the admin
//!   UI read/set/clear a key's rules.
//! - Internal ([`ServiceAuth`]-authed) routes under `/internal` let the gateway
//!   fetch rules for its cache and record spend off the response path.

pub mod db;
pub mod routes;
pub mod service_auth;
pub mod types;

pub use service_auth::ServiceAuth;

/// Canonicalize an `api_key_hash` path/param: a 0x-prefixed 32-byte (64 hex
/// char) keccak256 hash, normalized to lowercase. The operator UI and the
/// gateway must agree on this exact representation.
pub fn canonical_key_hash(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let body = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if body.len() != 64 || !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("api_key_hash must be a 0x-prefixed 32-byte hex string".into());
    }
    Ok(format!("0x{}", body.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::canonical_key_hash;

    #[test]
    fn normalizes_case_and_prefix() {
        let h = "ABCD".repeat(16); // 64 hex chars
        let with_prefix = format!("0x{h}");
        assert_eq!(
            canonical_key_hash(&with_prefix).unwrap(),
            format!("0x{}", h.to_ascii_lowercase())
        );
        // Accepts the unprefixed form too.
        assert_eq!(
            canonical_key_hash(&h).unwrap(),
            format!("0x{}", h.to_ascii_lowercase())
        );
    }

    #[test]
    fn rejects_wrong_length_or_non_hex() {
        assert!(canonical_key_hash("0x1234").is_err());
        assert!(canonical_key_hash(&"zz".repeat(32)).is_err());
    }
}
