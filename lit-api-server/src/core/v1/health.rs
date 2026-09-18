//! GET /health endpoint for NLB health checks.
//!
//! Returns 200 when healthy, 503 when unhealthy.  No authentication required —
//! intended for infrastructure probes (AWS NLB, k8s liveness, etc.).
//!
//! Only checks things that can fail *after* a successful startup.  Config,
//! chain clients, and signer pool are validated on boot and the process exits
//! if any of those fail, so they're guaranteed present when this endpoint is
//! reachable.  The lit-actions gRPC service, however, runs in a separate
//! container and can go down independently, and CPU load can spike at runtime.

use crate::actions::grpc::GrpcClientPool;
use crate::core::v1::guards::cpu_overload::CpuOverloadMonitor;
use crate::stripe::StripeState;
use lit_actions_grpc::unix;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{Route, State, get, routes};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const LIT_ACTIONS_SOCKET: &str = "/tmp/lit_actions.sock";
/// Default socket of the any-language (gVisor) runner. Overridable at boot via
/// the `LIT_ACTIONS_GVISOR_SOCKET` env var (prod mounts it under /var/run/lit).
pub const LIT_ACTIONS_GVISOR_SOCKET: &str = "/tmp/lit_actions_gvisor.sock";

/// Wrapper around the lit-actions socket path so it can be injected via
/// Rocket managed state. Tests build the rocket with a path guaranteed not to
/// exist so the reachability probe is hermetic; production wires in the real
/// `/tmp/lit_actions.sock`.
pub struct LitActionsSocketPath(pub PathBuf);

/// Socket of the any-language (gVisor) runner — the `/lit_binary_action`
/// backend. Injected the same way as `LitActionsSocketPath`.
pub struct LitActionsGvisorSocketPath(pub PathBuf);

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub lit_actions_reachable: bool,
    /// Reachability of the gVisor runner. Informational only — does NOT gate
    /// health status, so a node still reports healthy before the gVisor
    /// container is rolled out (or if the binary route is unused).
    pub lit_actions_gvisor_reachable: bool,
    pub cpu_available: bool,
    pub billing_keys_present: bool,
    /// Seconds since the on-chain account-event listener last confirmed it was
    /// current with the chain head, or `null` if it has not yet completed a poll.
    /// Healthy values sit at or below the ~10s poll interval; a large value means
    /// this instance's execute-path authorization cache is no longer being
    /// invalidated by on-chain writes, so a just-changed permission can take
    /// until the cache TTL (30s for denials, 5min for grants) to take effect on
    /// this instance. Informational only — does NOT affect the health status (a
    /// lagging listener still serves traffic; list/read endpoints stay live
    /// regardless). Clients (e.g. the dashboard) surface a staleness banner when
    /// this exceeds 30s.
    pub account_event_listener_lag_seconds: Option<u64>,
}

pub fn routes() -> Vec<Route> {
    routes![health]
}

/// GET /health — lightweight readiness probe for NLB / load balancer health checks.
#[get("/health")]
async fn health(
    grpc_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    cpu_monitor: &State<CpuOverloadMonitor>,
    stripe_state: &State<Option<Arc<StripeState>>>,
    socket_path: &State<LitActionsSocketPath>,
    gvisor_socket_path: &State<LitActionsGvisorSocketPath>,
) -> (Status, Json<HealthResponse>) {
    let lit_actions_reachable = probe_socket(grpc_pool, &socket_path.0).await;
    let lit_actions_gvisor_reachable = probe_socket(grpc_pool, &gvisor_socket_path.0).await;

    let cpu_available = !cpu_monitor.is_overloaded();
    let billing_keys_present = stripe_state.is_some();
    let account_event_listener_lag_seconds = crate::account_events::listener_lag_seconds();

    // Only lit_actions_reachable + cpu_available gate health.
    // billing_keys_present, lit_actions_gvisor_reachable, and
    // account_event_listener_lag_seconds are informational only.
    let healthy = lit_actions_reachable && cpu_available;

    let status = if healthy {
        Status::Ok
    } else {
        Status::ServiceUnavailable
    };

    (
        status,
        Json(HealthResponse {
            lit_actions_reachable,
            lit_actions_gvisor_reachable,
            cpu_available,
            billing_keys_present,
            account_event_listener_lag_seconds,
        }),
    )
}

/// Probe a lit-actions gRPC socket for reachability, reusing the shared pool.
///
/// If we already have a pooled connection, that's a cheap HashMap hit.
/// Otherwise try to connect (1s timeout via `connect_to_socket`); a success
/// also populates the pool so subsequent probes stay cheap. This mirrors the
/// pooling the execution path uses, so a healthy probe warms the same
/// connection real traffic reuses — avoiding the deadlock where the NLB marks
/// the node unhealthy, cuts traffic, and the lazy connection never forms.
async fn probe_socket(
    grpc_pool: &GrpcClientPool<tonic::transport::Channel>,
    socket_path: &Path,
) -> bool {
    let socket_key = socket_path.to_string_lossy();
    if grpc_pool.get_connection(&socket_key).await.is_some() {
        return true;
    }
    match unix::connect_to_socket(socket_path.to_path_buf()).await {
        Ok(channel) => {
            grpc_pool.add_connection(&socket_key, channel).await;
            true
        }
        Err(e) => {
            // Surface the connect error so /health failures are debuggable
            // without an exec into the container. /health is unauthenticated
            // and polled by the NLB on a short interval, so log at debug —
            // promoting to warn would flood logs (and on-call) during any
            // sustained lit-actions outage. The JSON response already signals
            // the failure; this line just records the *reason*.
            // Generic wording since this probes both runners (JS and gVisor);
            // the `socket` field identifies exactly which one failed.
            tracing::debug!(
                socket = %socket_key,
                error = %e,
                "runner socket connect failed during /health probe"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::grpc::GrpcClientPool;
    use crate::core::v1::guards::cpu_overload::CpuOverloadMonitor;
    use rocket::local::asynchronous::Client;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    /// Build a per-test socket path under `temp_dir()` keyed by PID + a
    /// monotonic counter, then assert it doesn't already exist. This keeps
    /// the lit-actions reachability probe hermetic across concurrent test
    /// runs and across machines where the previously-used hardcoded path
    /// might (in theory) be created by something else.
    fn unique_nonexistent_socket_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "lit_actions_test_{}_{}_{}.sock",
            std::process::id(),
            n,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        assert!(
            !path.exists(),
            "test socket path collided with existing file: {path:?}"
        );
        path
    }

    fn build_rocket(
        overloaded: bool,
        stripe_state: Option<Arc<StripeState>>,
    ) -> rocket::Rocket<rocket::Build> {
        let pool = GrpcClientPool::<tonic::transport::Channel>::new();
        let monitor = CpuOverloadMonitor::new_with_flag(Arc::new(AtomicBool::new(overloaded)));
        // Per-test path under temp_dir() so the lit-actions reachability probe
        // is hermetic — otherwise the test would inherit whatever
        // /tmp/lit_actions.sock happens to be on the host (e.g. a real
        // lit-actions process running for local dev), which would flip the
        // expected "unreachable" result to "reachable".
        let socket = LitActionsSocketPath(unique_nonexistent_socket_path());
        let gvisor_socket = LitActionsGvisorSocketPath(unique_nonexistent_socket_path());
        rocket::build()
            .manage(pool)
            .manage(monitor)
            .manage(stripe_state)
            .manage(socket)
            .manage(gvisor_socket)
            .mount("/", routes![health])
    }

    #[tokio::test]
    async fn health_returns_json_with_expected_shape() {
        let client = Client::tracked(build_rocket(false, None))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(body.cpu_available);
    }

    #[tokio::test]
    async fn health_always_serializes_listener_lag_field() {
        // The field is always present in the JSON (as a number or `null`) so
        // clients can rely on its key. We assert on the raw body rather than the
        // deserialized value because the lag is backed by a process-global the
        // account_events unit tests also exercise — asserting a specific value
        // here would race with them. (`Option` would silently deserialize a
        // missing key as `None`, so a deserialize round-trip can't prove the key
        // is emitted.)
        let client = Client::tracked(build_rocket(false, None))
            .await
            .expect("valid rocket");
        let body = client
            .get("/health")
            .dispatch()
            .await
            .into_string()
            .await
            .expect("body");
        assert!(
            body.contains("account_event_listener_lag_seconds"),
            "health body must always include the listener-lag key, got: {body}"
        );
    }

    #[tokio::test]
    async fn health_reports_lit_actions_unreachable_when_no_socket() {
        let client = Client::tracked(build_rocket(false, None))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.lit_actions_reachable);
    }

    #[tokio::test]
    async fn health_reports_gvisor_unreachable_when_no_socket() {
        // With no gVisor socket present the field reports false, but because it
        // is informational it must not, on its own, flip the status to 503
        // (that only happens here because the JS socket is also absent).
        let client = Client::tracked(build_rocket(false, None))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.lit_actions_gvisor_reachable);
    }

    #[tokio::test]
    async fn health_returns_503_when_cpu_overloaded() {
        let client = Client::tracked(build_rocket(true, None))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.cpu_available);
    }

    #[tokio::test]
    async fn health_billing_keys_present_false_when_no_stripe() {
        let client = Client::tracked(build_rocket(false, None))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.billing_keys_present);
    }

    #[tokio::test]
    async fn health_billing_keys_present_does_not_affect_status() {
        // billing_keys_present is purely informational — it never changes the
        // HTTP status.  With CPU overloaded the endpoint returns 503 regardless
        // of billing state.
        let client = Client::tracked(build_rocket(true, None))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.billing_keys_present);
        assert!(!body.cpu_available);
    }
}
