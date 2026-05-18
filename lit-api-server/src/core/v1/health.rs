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
use std::path::PathBuf;
use std::sync::Arc;

pub const LIT_ACTIONS_SOCKET: &str = "/tmp/lit_actions.sock";

/// Wrapper around the lit-actions socket path so it can be injected via
/// Rocket managed state. Tests build the rocket with a path guaranteed not to
/// exist so the reachability probe is hermetic; production wires in the real
/// `/tmp/lit_actions.sock`.
pub struct LitActionsSocketPath(pub PathBuf);

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub lit_actions_reachable: bool,
    pub cpu_available: bool,
    pub billing_keys_present: bool,
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
) -> (Status, Json<HealthResponse>) {
    let socket_key = socket_path.0.to_string_lossy();
    // Check if we have a pooled gRPC connection to lit-actions.  If not, try
    // to connect (1s timeout via connect_to_socket).  This avoids the deadlock
    // where NLB marks the node unhealthy → no traffic → lazy connection never
    // established → stays unhealthy.  A successful connect also populates the
    // pool so subsequent probes are a cheap HashMap lookup.
    let lit_actions_reachable = if grpc_pool.get_connection(&socket_key).await.is_some() {
        true
    } else {
        match unix::connect_to_socket(socket_path.0.clone()).await {
            Ok(channel) => {
                grpc_pool.add_connection(&socket_key, channel).await;
                true
            }
            Err(_) => false,
        }
    };

    let cpu_available = !cpu_monitor.is_overloaded();
    let billing_keys_present = stripe_state.is_some();

    // billing_keys_present is informational only — does NOT affect health status.
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
            cpu_available,
            billing_keys_present,
        }),
    )
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
        rocket::build()
            .manage(pool)
            .manage(monitor)
            .manage(stripe_state)
            .manage(socket)
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
