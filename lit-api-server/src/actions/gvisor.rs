//! Runtime gate for the gVisor any-language runner (CPL-359).
//!
//! gVisor is **off by default**. The two halves of the gate are independent:
//!
//!   * Build time — the runner binaries live behind the `gvisor` cargo feature
//!     on `lit-actions-gvisor-server`, so a default build never compiles them.
//!   * Run time — this flag. When gVisor is disabled the `/lit_binary_action`
//!     endpoint stays mounted (its OpenAPI surface is unchanged) but every call
//!     short-circuits with a "feature disabled" response before doing any work,
//!     so an api-server built without a gVisor runner alongside it degrades
//!     cleanly instead of dialing a socket that will never answer.
//!
//! A deploy that ships the gVisor runner opts in with `LIT_GVISOR_ENABLED`.

use crate::core::v1::helpers::api_status::ApiStatus;

/// Env var enabling the gVisor runner. Unset — or any value other than a
/// truthy token (`1`, `true`, `yes`, `on`, case-insensitive) — leaves it
/// disabled, so the default surface is gVisor-off.
pub const LIT_GVISOR_ENABLED_ENV: &str = "LIT_GVISOR_ENABLED";

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

    /// The guard every gVisor-backed endpoint calls first: `Ok(())` when the
    /// runner is enabled, otherwise the 503 "feature disabled" status the
    /// caller returns verbatim.
    pub fn ensure_enabled(self) -> Result<(), ApiStatus> {
        if self.enabled {
            Ok(())
        } else {
            Err(ApiStatus::service_unavailable(
                "The gVisor any-language runner is disabled on this node.",
            ))
        }
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
    use rocket::http::Status;

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

    #[test]
    fn ensure_enabled_gates_on_the_flag() {
        assert!(GvisorFeature::new(true).ensure_enabled().is_ok());

        let err = GvisorFeature::new(false).ensure_enabled().unwrap_err();
        assert_eq!(err.status, Status::ServiceUnavailable);
        assert!(err.message.contains("disabled"), "{}", err.message);
    }
}
