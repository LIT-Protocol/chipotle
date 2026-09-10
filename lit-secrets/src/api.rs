//! Shared API error/response helpers.

use rocket::http::Status;
use rocket::response::status::Custom;
use rocket::serde::json::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub type ApiError = Custom<Json<ErrorResponse>>;
pub type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn err(status: Status, error: &str) -> ApiError {
    Custom(
        status,
        Json(ErrorResponse {
            error: error.to_string(),
            detail: None,
        }),
    )
}

pub fn err_detail(status: Status, error: &str, detail: impl Into<String>) -> ApiError {
    Custom(
        status,
        Json(ErrorResponse {
            error: error.to_string(),
            detail: Some(detail.into()),
        }),
    )
}

/// Log the underlying error server-side and return an opaque 500 to the client.
pub fn internal(context: &str, e: impl std::fmt::Display) -> ApiError {
    tracing::warn!("{context}: {e}");
    err(Status::InternalServerError, context)
}

/// A Chipotle upstream failure: surfaced as 502 with the upstream message so
/// operators can tell "our bug" from "Chipotle said no" (e.g. 402 unfunded).
pub fn upstream(context: &str, e: &crate::chipotle::ChipotleError) -> ApiError {
    tracing::warn!("{context}: {e}");
    err_detail(Status::BadGateway, context, e.to_string())
}
