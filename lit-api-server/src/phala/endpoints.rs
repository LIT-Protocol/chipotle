//! Phala attestation endpoints.

#![cfg(feature = "phala")]

use crate::core::models::{ApiResult, ApiStatus, ErrMessage};
use http_client_unix_domain_socket::{ClientUnix, Method};
use rocket::{Route, get, routes};
use rocket_responder::ApiResponse;
use serde::{Deserialize, Serialize};

const DSTACK_SOCKET_PATH: &str = "/var/run/dstack.sock";

/// Response for the verify endpoint: TDX quote from Phala/dstack.
#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    /// TDX attestation quote (hex).
    pub quote: String,
    /// Event log (hex).
    pub event_log: String,
    /// Report data included in the quote (hex).
    pub report_data: String,
    /// VM configuration (required by dstack-verifier).
    pub vm_config: String,
}

#[derive(Serialize)]
struct GetQuoteRequest {
    report_data: String,
}

#[derive(Deserialize)]
struct GetQuoteResponse {
    quote: String,
    event_log: String,
    report_data: String,
    vm_config: String,
}

#[get("/verify")]
async fn verify() -> ApiResponse<VerifyResponse, ErrMessage> {
    ApiResult(verify_inner().await).into()
}

async fn verify_inner() -> Result<VerifyResponse, ApiStatus> {
    let socket_path = std::env::var("DSTACK_SIMULATOR_ENDPOINT")
        .unwrap_or_else(|_| DSTACK_SOCKET_PATH.to_string());

    if !std::path::Path::new(&socket_path).exists() {
        return Err(ApiStatus::internal_server_error(
            anyhow::anyhow!("dstack socket not found at {}", socket_path),
            "Phala attestation unavailable (not running in Phala CVM)",
        ));
    }

    let mut client = ClientUnix::try_new(&socket_path).await.map_err(|e| {
        ApiStatus::internal_server_error(
            anyhow::anyhow!("Failed to connect to dstack: {:?}", e),
            "Phala attestation unavailable",
        )
    })?;

    // Report data: 32 zero bytes (no custom data; dstack requires 1–64 bytes)
    let report_data = hex::encode([0u8; 32]);
    let request = GetQuoteRequest { report_data };

    let (_, response) = client
        .send_request_json::<GetQuoteRequest, GetQuoteResponse, serde_json::Value>(
            "/GetQuote",
            Method::POST,
            &[("Content-Type", "application/json"), ("Host", "dstack")],
            Some(&request),
        )
        .await
        .map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!("Failed to obtain TDX quote: {:?}", e),
                "Phala attestation unavailable",
            )
        })?;

    Ok(VerifyResponse {
        quote: response.quote,
        event_log: response.event_log,
        report_data: response.report_data,
        vm_config: response.vm_config,
    })
}

pub fn routes() -> Vec<Route> {
    routes![verify]
}
