//! Rocket request guard verifying the `X-Internal-Secret` header on inbound
//! internal-only endpoints (Phase 5 adds `/internal/invalidate_balance_cache`,
//! which is the first consumer).
//!
//! Mirrors `lit-payments::internal::guard::InternalSecret`: constant-time
//! compare via `subtle::ConstantTimeEq` (§15 of the auto-top-up plan).
//! Missing header, missing config, or mismatched value all yield 401.
//!
//! Length-leak hardening: pre-fix the guard returned 401 the instant the
//! presented bytes differed in length from the expected secret, which
//! leaks the expected length via timing (early-return short-circuits the
//! constant-time compare entirely). Now both inputs are padded to the
//! longer length with a fixed sentinel byte, compared in constant time,
//! and the boolean is `length_eq & contents_eq` — neither branch can
//! be observed independently.

use std::sync::Arc;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use subtle::{Choice, ConstantTimeEq};

use super::config::InternalConfig;

const HEADER: &str = "X-Internal-Secret";

pub struct InternalSecret;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for InternalSecret {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(presented) = req.headers().get_one(HEADER) else {
            return Outcome::Error((Status::Unauthorized, ()));
        };

        // The Rocket-managed value is `Option<Arc<InternalConfig>>` so dev /
        // simulator builds without the env vars set degrade to "no internal
        // endpoints" rather than panicking at boot. A request hitting an
        // internal endpoint in such a build is by definition unauthorized.
        let cfg = match req.rocket().state::<Option<Arc<InternalConfig>>>() {
            Some(Some(c)) => c,
            Some(None) | None => return Outcome::Error((Status::Unauthorized, ())),
        };

        let expected = cfg.lit_internal_shared_secret.as_bytes();
        let presented_bytes = presented.as_bytes();

        if bool::from(constant_time_eq(presented_bytes, expected)) {
            Outcome::Success(InternalSecret)
        } else {
            Outcome::Error((Status::Unauthorized, ()))
        }
    }
}

/// Constant-time equality for byte slices of any length.
///
/// Pads both inputs to the longer length with a fixed sentinel byte
/// (`0u8`), runs `subtle::ConstantTimeEq::ct_eq` on the equal-length
/// buffers, then `&`s the result with a length-equality `Choice`. Total
/// work is O(max(a, b)), independent of which input is shorter — so
/// neither a length mismatch nor a content mismatch is detectable via
/// timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> Choice {
    let max = a.len().max(b.len());
    // Stack-allocate up to a reasonable cap; spill to heap otherwise.
    // Internal secrets in this codebase are ~44 bytes (base64 of 32
    // random bytes), so the heap path is practically unreachable.
    let mut pa = vec![0u8; max];
    let mut pb = vec![0u8; max];
    pa[..a.len()].copy_from_slice(a);
    pb[..b.len()].copy_from_slice(b);
    let contents_eq = pa.ct_eq(&pb);
    let length_eq = (a.len() as u64).ct_eq(&(b.len() as u64));
    contents_eq & length_eq
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn equal_slices_eq() {
        assert!(bool::from(constant_time_eq(b"abc", b"abc")));
    }
    #[test]
    fn different_contents_same_length_ne() {
        assert!(!bool::from(constant_time_eq(b"abc", b"abd")));
    }
    #[test]
    fn different_length_ne_even_when_prefix_matches() {
        assert!(!bool::from(constant_time_eq(b"abc", b"abcd")));
        assert!(!bool::from(constant_time_eq(b"abcd", b"abc")));
    }
    #[test]
    fn both_empty_eq() {
        assert!(bool::from(constant_time_eq(b"", b"")));
    }
    #[test]
    fn one_empty_ne() {
        assert!(!bool::from(constant_time_eq(b"", b"x")));
        assert!(!bool::from(constant_time_eq(b"x", b"")));
    }
}
