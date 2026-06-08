//! Rocket request guard verifying the `X-Internal-Secret` header on inbound
//! internal-only endpoints (Phase 5 adds `/internal/invalidate_balance_cache`,
//! which is the first consumer).
//!
//! Mirrors `lit-payments::internal::guard::InternalSecret`: constant-time
//! compare via `subtle::ConstantTimeEq` (§14 of the auto-top-up plan).
//! Missing header, missing config, or mismatched value all yield 401.

use std::sync::Arc;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use subtle::ConstantTimeEq;

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

        if presented_bytes.len() != expected.len() {
            return Outcome::Error((Status::Unauthorized, ()));
        }

        if bool::from(presented_bytes.ct_eq(expected)) {
            Outcome::Success(InternalSecret)
        } else {
            Outcome::Error((Status::Unauthorized, ()))
        }
    }
}
