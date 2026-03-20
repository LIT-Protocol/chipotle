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
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{Route, State, get, routes};
use serde::{Deserialize, Serialize};
use std::path::Path;

const LIT_ACTIONS_SOCKET: &str = "/tmp/lit_actions.sock";

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub lit_actions_reachable: bool,
    pub cpu_overloaded: bool,
}

pub fn routes() -> Vec<Route> {
    routes![health]
}

/// GET /health — lightweight readiness probe for NLB / load balancer health checks.
#[get("/health")]
async fn health(
    grpc_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    cpu_monitor: &State<CpuOverloadMonitor>,
) -> (Status, Json<HealthResponse>) {
    // Check if we already have a pooled gRPC connection to lit-actions, or if
    // the socket file at least exists.  Avoids opening a new connection on
    // every NLB probe; the first real request will populate the pool.
    let lit_actions_reachable = if grpc_pool.get_connection(LIT_ACTIONS_SOCKET).await.is_some() {
        true
    } else {
        Path::new(LIT_ACTIONS_SOCKET).exists()
    };

    let cpu_overloaded = cpu_monitor.is_overloaded();

    let healthy = lit_actions_reachable && !cpu_overloaded;

    let status = if healthy {
        Status::Ok
    } else {
        Status::ServiceUnavailable
    };

    (
        status,
        Json(HealthResponse {
            healthy,
            lit_actions_reachable,
            cpu_overloaded,
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
    use std::sync::atomic::AtomicBool;

    fn build_rocket(overloaded: bool) -> rocket::Rocket<rocket::Build> {
        let pool = GrpcClientPool::<tonic::transport::Channel>::new();
        let monitor = CpuOverloadMonitor::new_with_flag(Arc::new(AtomicBool::new(overloaded)));
        rocket::build()
            .manage(pool)
            .manage(monitor)
            .mount("/", routes![health])
    }

    #[tokio::test]
    async fn health_returns_json_with_expected_shape() {
        let client = Client::tracked(build_rocket(false))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert_eq!(
            body.healthy,
            body.lit_actions_reachable && !body.cpu_overloaded
        );
    }

    #[tokio::test]
    async fn health_reports_lit_actions_unreachable_when_no_socket() {
        let client = Client::tracked(build_rocket(false))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.lit_actions_reachable);
        assert!(!body.healthy);
    }

    #[tokio::test]
    async fn health_returns_503_when_cpu_overloaded() {
        let client = Client::tracked(build_rocket(true))
            .await
            .expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(body.cpu_overloaded);
        assert!(!body.healthy);
    }
}
