//! Runtime gate for the gVisor any-language runner (CPL-359).
//!
//! gVisor is **off by default**. The two halves of the gate are independent:
//!
//!   * Build time — the runner binaries live behind the `gvisor` cargo feature
//!     on `lit-actions-gvisor-server`, so a default build never compiles them.
//!   * Run time — this flag. When gVisor is disabled the `/lit_binary_action`
//!     endpoint stays mounted (its OpenAPI surface is unchanged) but the
//!     [`GvisorEnabled`] request guard rejects every call with a "feature
//!     disabled" 503 *before* the CPU and billing guards run — so a disabled
//!     node sheds the call without a Stripe credit check or any other work,
//!     rather than dialing a socket that will never answer.
//!
//! A deploy that ships the gVisor runner opts in with `LIT_GVISOR_ENABLED`.

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

use crate::core::v1::catchers::set_error_detail;

/// Env var enabling the gVisor runner. Unset — or any value other than a
/// truthy token (`1`, `true`, `yes`, `on`, case-insensitive) — leaves it
/// disabled, so the default surface is gVisor-off.
pub const LIT_GVISOR_ENABLED_ENV: &str = "LIT_GVISOR_ENABLED";

/// Message returned when a gVisor-backed endpoint is called on a node that has
/// the runner disabled.
const DISABLED_MESSAGE: &str = "The gVisor any-language runner is disabled on this node.";
const DISABLED_FIX: &str = "This node runs JavaScript actions only (POST /lit_action). Send \
     binary/any-language actions to a node that advertises gVisor languages via \
     GET /get_supported_languages.";

/// Whether the gVisor any-language runner is enabled on this node. Rocket
/// managed state, built once at startup from [`LIT_GVISOR_ENABLED_ENV`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GvisorFeature {
    enabled: bool,
}

impl GvisorFeature {
    /// Reads [`LIT_GVISOR_ENABLED_ENV`]. Absent or non-truthy => disabled.
    pub fn from_env() -> Self {
        Self {
            enabled: parse_enabled(std::env::var(LIT_GVISOR_ENABLED_ENV).ok().as_deref()),
        }
    }

    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }
}

/// Request guard for gVisor-backed endpoints. Reads [`GvisorFeature`] from
/// managed state and rejects with `503` + a "feature disabled" detail when the
/// runner is off. Absent state fails closed (disabled).
///
/// Place it as the **first** handler parameter so the gate is evaluated before
/// the CPU-overload and billing guards — a disabled node then never reaches the
/// Stripe credit check in `BilledLitActionApiKey`.
pub struct GvisorEnabled;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for GvisorEnabled {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let enabled = req
            .rocket()
            .state::<GvisorFeature>()
            .is_some_and(|f| f.enabled());
        if enabled {
            Outcome::Success(GvisorEnabled)
        } else {
            // Populate the request-local detail the 503 catcher renders as JSON.
            set_error_detail(req, DISABLED_MESSAGE, DISABLED_FIX);
            Outcome::Error((Status::ServiceUnavailable, ()))
        }
    }
}

impl<'r> OpenApiFromRequest<'r> for GvisorEnabled {
    fn from_request_input(
        _generator: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        // Internal guard — not a user-visible parameter, and it adds no
        // response schema (keeps the generated OpenAPI/k6 client unchanged).
        Ok(RequestHeaderInput::None)
    }
}

/// Truthy tokens mirror the common shell/env convention; everything else
/// (including a bare, unset var) is off.
fn parse_enabled(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};

    #[test]
    fn truthy_tokens_enable() {
        for token in ["1", "true", "TRUE", "  Yes  ", "on"] {
            assert!(parse_enabled(Some(token)), "{token:?} should enable");
        }
    }

    #[test]
    fn everything_else_disables() {
        for token in [
            None,
            Some(""),
            Some("0"),
            Some("false"),
            Some("no"),
            Some("enable"),
        ] {
            assert!(!parse_enabled(token), "{token:?} should stay disabled");
        }
    }

    #[get("/gated")]
    fn gated_route(_gvisor: GvisorEnabled) -> &'static str {
        "ok"
    }

    async fn status_for(feature: Option<GvisorFeature>) -> Status {
        let mut rocket = rocket::build().mount("/", routes![gated_route]);
        if let Some(feature) = feature {
            rocket = rocket.manage(feature);
        }
        let client = Client::tracked(rocket).await.expect("valid rocket");
        client.get("/gated").dispatch().await.status()
    }

    #[tokio::test]
    async fn guard_passes_when_enabled() {
        assert_eq!(status_for(Some(GvisorFeature::new(true))).await, Status::Ok);
    }

    #[tokio::test]
    async fn guard_rejects_when_disabled() {
        assert_eq!(
            status_for(Some(GvisorFeature::new(false))).await,
            Status::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn guard_fails_closed_when_state_absent() {
        // No GvisorFeature managed => treat as disabled, never as enabled.
        assert_eq!(status_for(None).await, Status::ServiceUnavailable);
    }
}
