//! Shared billing auth primitives for `lit-api-server` and `lit-payments`.
//!
//! The dashboard and external API clients authenticate with either:
//!
//! 1. **API key** (`Authorization: Bearer <key>` or `X-Api-Key: <key>`) — the
//!    legacy path. The raw key string is the identity until something resolves
//!    it to a wallet (and from there to a Stripe customer).
//!
//! 2. **Wallet signature** — an EIP-712 typed-data payload, base64-JSON-encoded
//!    in the `X-Wallet-Auth` header, with `primaryType: "BillingAuth"`.
//!    Recovering the signer proves wallet possession.
//!
//! Both services need the same identity model. The Rocket request guard
//! [`BillingAuth`] lives here; per-service plumbing (on-chain key resolver,
//! local EIP-712 verifier, or an HTTP hop to lit-api-server) is supplied
//! via the [`AuthResolver`] trait that handlers pull from Rocket state.
//!
//! Heavy implementation details (EIP-712 schema validation, on-chain
//! `allApiKeyHashesToMaster` lookups) live in `lit-api-server` and are not
//! re-implemented here — `lit-payments` reaches them via internal endpoints.

pub mod guard;
pub mod resolver;

pub use guard::BillingAuth;
pub use resolver::{AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload};

/// True when `s` is shaped like a precomputed 32-byte keccak256 hash:
/// lowercase 0x-prefixed, exactly 66 chars, hex body.
///
/// `BillingAuth::FromRequest` rejects `X-Api-Key` / `Authorization: Bearer`
/// values matching this shape — those must come through the verified
/// `WalletSigned` path only. Otherwise an attacker could send
/// `X-Api-Key: 0x{keccak256(walletAddress)}` and short-circuit the EIP-712
/// signature check (CPL-285 / CPL-286 hardening).
///
/// Self-contained: no on-chain or service-specific dependencies, so it lives
/// in the shared crate rather than each service's `parse_with_hash.rs`.
pub fn is_precomputed_hash_shape(s: &str) -> bool {
    let trimmed = s.trim();
    if !(trimmed.starts_with("0x") && trimmed.len() == 66) {
        return false;
    }
    trimmed[2..].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_precomputed_hash_shape() {
        let wallet_hash = "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert!(is_precomputed_hash_shape(wallet_hash));
    }

    #[test]
    fn rejects_real_api_key_shape() {
        let api_key = "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU2Nzg5";
        assert!(!is_precomputed_hash_shape(api_key));
    }

    #[test]
    fn rejects_uppercase_prefix() {
        let upper = "0Xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert!(!is_precomputed_hash_shape(upper));
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(!is_precomputed_hash_shape("0xabcdef"));
        assert!(!is_precomputed_hash_shape(
            "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789ff"
        ));
    }

    #[test]
    fn rejects_non_hex_body() {
        let not_hex = "0xZZcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert!(!is_precomputed_hash_shape(not_hex));
    }
}
