use rocket::serde::json::Json;
use rocket::{Route, get, routes};
use serde::Serialize;

use super::dstack;
use dstack_sdk::dstack_client;

/// Response for GET /attestation — per [Phala Get Attestation](https://docs.phala.com/phala-cloud/attestation/get-attestation).
/// Verifiers fetch this to validate the CVM runs in genuine TEE hardware.
#[derive(Debug, Serialize)]
pub struct AttestationResponse {
    pub quote: String,
    pub event_log: String,
    pub vm_config: String,
}

pub fn routes() -> Vec<Route> {
    routes![attestation, info]
}

/// Routes for root-level attestation (GET /attestation) and info (GET /info). Mount at "/".
pub fn attestation_routes() -> Vec<Route> {
    routes![attestation, info]
}

/// GET /attestation — returns quote, event_log, vm_config for CVM verification.
/// The app fetches from the dstack socket; the gateway cannot serve attestation.
#[get("/attestation")]
async fn attestation() -> Result<Json<AttestationResponse>, (rocket::http::Status, String)> {
    match dstack::get_quote(None).await {
        Ok(q) => Ok(Json(AttestationResponse {
            quote: q.quote,
            event_log: q.event_log,
            vm_config: q.vm_config,
        })),
        Err(e) => Err((rocket::http::Status::ServiceUnavailable, e)),
    }
}

/// GET /info — returns app info (app_id, instance_id, tcb_info, compose_hash, etc.) per
/// [Phala Get Attestation](https://docs.phala.com/phala-cloud/attestation/get-attestation).
/// Verifiers use tcb_info.app_compose and compose_hash for compose-hash verification.
#[get("/info")]
async fn info() -> Result<Json<dstack_client::InfoResponse>, (rocket::http::Status, String)> {
    match dstack::get_info().await {
        Ok(i) => Ok(Json(i)),
        Err(e) => Err((rocket::http::Status::ServiceUnavailable, e)),
    }
}
