//! [`SpendingContext`] — the per-request facts the spending-rules enforcer
//! needs beyond the API key: the browser `Origin` header (origin allowlist) and
//! the client IP (per-IP rate limit).
//!
//! Infallible: it never rejects a request on its own. Enforcement (and the
//! decision to fail closed on a missing `Origin`) lives in
//! [`crate::core::spending_rules`], and only runs for keys whose on-chain
//! `hasSpendingRules` flag is set.
//!
//! The client IP comes from [`rocket::Request::client_ip`], i.e. Rocket's
//! configured `ip_header` (default `X-Real-IP`) with a socket-peer fallback.
//! See the trust-model note in [`super::rate_limit`]: behind the dstack ingress
//! the per-IP key is only meaningful if the proxy overwrites that header.

use std::net::IpAddr;

use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

/// Request facts consumed by `SpendingRulesState::admit`.
#[derive(Debug, Clone, Default)]
pub struct SpendingContext {
    /// Raw `Origin` header, if the client sent one. Browsers always do for
    /// cross-origin `fetch`; curl/servers usually do not.
    pub origin: Option<String>,
    /// Best-effort client address (see module docs for the trust model).
    pub client_ip: Option<IpAddr>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for SpendingContext {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(SpendingContext {
            origin: req
                .headers()
                .get_one("Origin")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            client_ip: req.client_ip(),
        })
    }
}

impl<'r> OpenApiFromRequest<'r> for SpendingContext {
    fn from_request_input(
        _generator: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        // `Origin` is set by the browser, not the caller — not a documented param.
        Ok(RequestHeaderInput::None)
    }
}
