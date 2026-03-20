//! GET /health endpoint for NLB health checks.
//!
//! Returns 200 with a JSON body when the server is healthy, or 503 when any
//! subsystem check fails.  No authentication is required — this is intended
//! for infrastructure probes (AWS NLB, k8s liveness, etc.).
//!
//! Healthy is defined as:
//! - Configuration initialized (NodeConfig loaded, chain accessible)
//! - Connected to a lit-actions gRPC service (Unix socket reachable)

use crate::actions::grpc::GrpcClientPool;
use crate::config::GLOBAL_NODE_CONFIG;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{Route, State, get, routes};
use serde::{Deserialize, Serialize};
use std::path::Path;

const LIT_ACTIONS_SOCKET: &str = "/tmp/lit_actions.sock";

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub config_initialized: bool,
    pub lit_actions_reachable: bool,
}

pub fn routes() -> Vec<Route> {
    routes![health]
}

/// GET /health — lightweight readiness probe for NLB / load balancer health checks.
#[get("/health")]
async fn health(
    grpc_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
) -> (Status, Json<HealthResponse>) {
    let config_initialized = GLOBAL_NODE_CONFIG.get().is_some();

    // Check if we already have a connection to lit-actions in the pool, or if
    // the socket file at least exists (avoids creating a new connection on
    // every health probe).
    let lit_actions_reachable = if grpc_pool.get_connection(LIT_ACTIONS_SOCKET).await.is_some() {
        true
    } else {
        // No pooled connection yet — fall back to checking the socket file
        // exists.  A full gRPC connect on every NLB probe would be wasteful;
        // the first real request will populate the pool.
        Path::new(LIT_ACTIONS_SOCKET).exists()
    };

    let healthy = config_initialized && lit_actions_reachable;

    let status = if healthy {
        Status::Ok
    } else {
        Status::ServiceUnavailable
    };

    (
        status,
        Json(HealthResponse {
            healthy,
            config_initialized,
            lit_actions_reachable,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::grpc::GrpcClientPool;
    use rocket::local::asynchronous::Client;

    fn build_rocket() -> rocket::Rocket<rocket::Build> {
        let pool = GrpcClientPool::<tonic::transport::Channel>::new();
        rocket::build().manage(pool).mount("/", routes![health])
    }

    #[tokio::test]
    async fn health_returns_json_with_expected_shape() {
        let client = Client::tracked(build_rocket()).await.expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        let body: HealthResponse = response.into_json().await.expect("valid json");
        // healthy is only true when ALL checks pass.
        assert_eq!(
            body.healthy,
            body.config_initialized && body.lit_actions_reachable
        );
    }

    #[tokio::test]
    async fn health_reports_lit_actions_unreachable_when_no_socket() {
        // In the test environment there is no /tmp/lit_actions.sock and no
        // pooled gRPC connection, so lit_actions_reachable should be false.
        let client = Client::tracked(build_rocket()).await.expect("valid rocket");
        let response = client.get("/health").dispatch().await;
        let body: HealthResponse = response.into_json().await.expect("valid json");
        assert!(!body.lit_actions_reachable);
    }
}
