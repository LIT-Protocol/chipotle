//! Attestation endpoints — same machinery as the core server
//! (lit-api-server/src/dstack/v1/endpoints.rs), plus the nonce binding the
//! core endpoint lacks: `GET /attestation?nonce=<hex>` binds the TDX quote's
//! report_data to a verifier-supplied nonce.

use rocket::serde::json::Json;
use rocket::{get, routes, Route};
use serde::Serialize;

const DSTACK_SOCKET_DEFAULT: &str = "/var/run/dstack.sock";

fn resolve_socket_path() -> String {
    #[cfg(feature = "production")]
    {
        DSTACK_SOCKET_DEFAULT.to_string()
    }
    #[cfg(not(feature = "production"))]
    {
        std::env::var("DSTACK_SOCKET").unwrap_or_else(|_| DSTACK_SOCKET_DEFAULT.to_string())
    }
}

fn client() -> Result<dstack_sdk::dstack_client::DstackClient, String> {
    let socket_path = resolve_socket_path();
    if !std::path::Path::new(&socket_path).exists() {
        return Err(format!("dstack socket not found at {socket_path}"));
    }
    Ok(dstack_sdk::dstack_client::DstackClient::new(Some(
        Box::leak(socket_path.into_boxed_str()),
    )))
}

#[derive(Debug, Serialize)]
pub struct AttestationResponse {
    pub quote: String,
    pub event_log: String,
    pub vm_config: String,
}

pub fn routes() -> Vec<Route> {
    routes![attestation, info]
}

#[get("/attestation?<nonce>")]
async fn attestation(
    nonce: Option<String>,
) -> Result<Json<AttestationResponse>, (rocket::http::Status, String)> {
    let report_data: Vec<u8> = match nonce.as_deref() {
        None | Some("") | Some("0x") => vec![0u8; 64],
        Some(s) => {
            let hex_str = s.strip_prefix("0x").unwrap_or(s);
            match hex::decode(hex_str) {
                Ok(bytes) if bytes.len() <= 64 => bytes,
                _ => {
                    return Err((
                        rocket::http::Status::BadRequest,
                        "nonce must be hex, at most 64 bytes".to_string(),
                    ))
                }
            }
        }
    };
    let client = client().map_err(|e| (rocket::http::Status::ServiceUnavailable, e))?;
    match client.get_quote(report_data).await {
        Ok(q) => Ok(Json(AttestationResponse {
            quote: q.quote,
            event_log: q.event_log,
            vm_config: q.vm_config,
        })),
        Err(e) => Err((
            rocket::http::Status::ServiceUnavailable,
            format!("failed to get quote: {e}"),
        )),
    }
}

#[get("/info")]
async fn info(
) -> Result<Json<dstack_sdk::dstack_client::InfoResponse>, (rocket::http::Status, String)> {
    let client = client().map_err(|e| (rocket::http::Status::ServiceUnavailable, e))?;
    match client.info().await {
        Ok(i) => Ok(Json(i)),
        Err(e) => Err((
            rocket::http::Status::ServiceUnavailable,
            format!("failed to get info: {e}"),
        )),
    }
}
