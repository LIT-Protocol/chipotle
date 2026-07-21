//! Gate for the gVisor any-language runner (CPL-359, CPL-361).
//!
//! gVisor is **off by default** and enabled only when **all three** independent
//! axes opt in — a call to `/lit_binary_action` succeeds past this gate iff:
//!
//!   1. Build time — the runner binaries live behind the `gvisor` cargo feature
//!      on `lit-actions-gvisor-server`, so a default build never compiles them.
//!   2. Run time (process) — the [`LIT_GVISOR_ENABLED_ENV`] env var. Defaulted
//!      **on** for testing/manual deploys and **off** for production by the
//!      deploy config (`docker-compose.phala.yml` + the deploy workflows).
//!   3. Run time (on-chain) — the [`GVISOR_RUNNER_ENABLED`](ConfigKeys::GVISOR_RUNNER_ENABLED)
//!      key in the contract's `nodeConfigurationValues` map (CPL-361). This lets
//!      an operator flip the runner off/on network-wide **without redeploying**
//!      the binary: even a built, env-enabled node stays gated until the
//!      contract opts in. Read through the cached [`ChainConfig`] snapshot.
//!
//! When any axis is off the endpoint stays mounted (its OpenAPI surface is
//! unchanged) but the [`GvisorEnabled`] request guard rejects every call with a
//! "feature disabled" 503 *before* the CPU and billing guards run — so a
//! disabled node sheds the call without a Stripe credit check or any other work,
//! rather than dialing a socket that will never answer. All axes **fail closed**:
//! absent state or a non-truthy value leaves the runner disabled.

use std::sync::Arc;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

use crate::accounts::chain_config::{ChainConfig, ConfigKeys};
use crate::core::v1::catchers::set_error_detail;

/// Env var enabling the gVisor runner. Unset — or any value other than a
/// truthy token (`1`, `true`, `yes`, `on`, case-insensitive) — leaves it
/// disabled, so the default surface is gVisor-off.
pub const LIT_GVISOR_ENABLED_ENV: &str = "LIT_GVISOR_ENABLED";

/// Message returned when a gVisor-backed endpoint is called on a node that has
/// the runner disabled at the process level (env off / not built).
const DISABLED_MESSAGE: &str = "The gVisor any-language runner is disabled on this node.";
const DISABLED_FIX: &str = "This node runs JavaScript actions only (POST /lit_action). Send \
     binary/any-language actions to a node that advertises gVisor languages via \
     GET /get_supported_languages.";

/// Message returned when the node has the runner built + env-enabled but the
/// on-chain contract gate ([`GVISOR_RUNNER_ENABLED`](ConfigKeys::GVISOR_RUNNER_ENABLED))
/// is off. Distinct from [`DISABLED_MESSAGE`] so operators can tell a
/// node-level opt-out apart from a network-wide contract flip (CPL-361).
const CONTRACT_DISABLED_MESSAGE: &str =
    "The gVisor any-language runner is disabled network-wide by contract configuration.";
const CONTRACT_DISABLED_FIX: &str = "This is a transient, network-wide setting toggled on-chain \
     via the GVISOR_RUNNER_ENABLED node configuration value; retry later or contact an operator.";

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

/// Request guard for gVisor-backed endpoints. Both run-time axes must opt in:
/// the process-level [`GvisorFeature`] (env) **and** the on-chain contract gate
/// read from [`ChainConfig`]. Either off => `503` + a "feature disabled" detail.
/// Absent state fails closed (disabled) on both axes.
///
/// Place it as the **first** handler parameter so the gate is evaluated before
/// the CPU-overload and billing guards — a disabled node then never reaches the
/// Stripe credit check in `BilledLitActionApiKey`.
pub struct GvisorEnabled;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for GvisorEnabled {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // Process-level (env) gate first — cheap and the most common opt-out.
        let env_enabled = req
            .rocket()
            .state::<GvisorFeature>()
            .is_some_and(|f| f.enabled());
        if !env_enabled {
            // Populate the request-local detail the 503 catcher renders as JSON.
            set_error_detail(req, DISABLED_MESSAGE, DISABLED_FIX);
            return Outcome::Error((Status::ServiceUnavailable, ()));
        }

        // On-chain contract gate (CPL-361). Reads the cached ChainConfig
        // snapshot; absent state or a non-truthy value fails closed.
        let contract_enabled = match req.rocket().state::<Arc<ChainConfig>>() {
            Some(cfg) => contract_gate_enabled(cfg).await,
            None => false,
        };
        if !contract_enabled {
            set_error_detail(req, CONTRACT_DISABLED_MESSAGE, CONTRACT_DISABLED_FIX);
            return Outcome::Error((Status::ServiceUnavailable, ()));
        }

        Outcome::Success(GvisorEnabled)
    }
}

/// Reads the on-chain [`GVISOR_RUNNER_ENABLED`](ConfigKeys::GVISOR_RUNNER_ENABLED)
/// value from the cached config snapshot and maps it through [`parse_enabled`].
/// Absent key or non-truthy value => `false` (fail closed).
///
/// `ChainConfig::get` is `async` for source compatibility but reads a lock-free
/// `ArcSwap` snapshot with no I/O, so this adds nothing to the request latency.
async fn contract_gate_enabled(cfg: &ChainConfig) -> bool {
    cfg.get(ConfigKeys::GVISOR_RUNNER_ENABLED)
        .await
        .ok()
        .flatten()
        .is_some_and(|v| parse_enabled(Some(&v)))
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

    /// Builds a Rocket with the given env feature and optional on-chain
    /// contract value for `GVISOR_RUNNER_ENABLED`, then dispatches the gated
    /// route and returns the resulting status. `chain_value: None` => no
    /// `ChainConfig` managed at all (state-absent case); `Some(v)` => a
    /// `ChainConfig` carrying that raw value under the gate key.
    async fn status_for(feature: Option<GvisorFeature>, chain_value: Option<&str>) -> Status {
        use crate::accounts::chain_config::from_pairs_for_test;
        use std::sync::Arc;

        let mut rocket = rocket::build().mount("/", routes![gated_route]);
        if let Some(feature) = feature {
            rocket = rocket.manage(feature);
        }
        if let Some(value) = chain_value {
            let cfg = from_pairs_for_test(&[("GVISOR_RUNNER_ENABLED", value)]);
            rocket = rocket.manage(Arc::new(cfg));
        }
        let client = Client::tracked(rocket).await.expect("valid rocket");
        client.get("/gated").dispatch().await.status()
    }

    #[tokio::test]
    async fn guard_passes_when_env_and_contract_enabled() {
        assert_eq!(
            status_for(Some(GvisorFeature::new(true)), Some("true")).await,
            Status::Ok
        );
    }

    #[tokio::test]
    async fn guard_rejects_when_env_disabled() {
        // Env off short-circuits before the contract gate, even if contract is on.
        assert_eq!(
            status_for(Some(GvisorFeature::new(false)), Some("true")).await,
            Status::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn guard_rejects_when_contract_disabled() {
        // Env on, but the on-chain gate is off => disabled (CPL-361).
        assert_eq!(
            status_for(Some(GvisorFeature::new(true)), Some("false")).await,
            Status::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn guard_rejects_when_contract_value_non_truthy() {
        // Env on, gate key present but empty (a non-truthy value) => disabled.
        assert_eq!(
            status_for(Some(GvisorFeature::new(true)), Some("")).await,
            Status::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn guard_fails_closed_when_contract_key_absent() {
        // Env on, ChainConfig present but the gate key is not among its values.
        use crate::accounts::chain_config::from_pairs_for_test;
        use std::sync::Arc;

        let rocket = rocket::build()
            .mount("/", routes![gated_route])
            .manage(GvisorFeature::new(true))
            .manage(Arc::new(from_pairs_for_test(&[("SOME_OTHER_KEY", "true")])));
        let client = Client::tracked(rocket).await.expect("valid rocket");
        assert_eq!(
            client.get("/gated").dispatch().await.status(),
            Status::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn guard_fails_closed_when_chain_config_absent() {
        // Env on, but no ChainConfig managed at all => fail closed.
        assert_eq!(
            status_for(Some(GvisorFeature::new(true)), None).await,
            Status::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn guard_fails_closed_when_env_state_absent() {
        // No GvisorFeature managed => treat as disabled, never as enabled.
        assert_eq!(
            status_for(None, Some("true")).await,
            Status::ServiceUnavailable
        );
    }
}
