use rocket::{http::Status, response::status::Custom, serde::json::Json};
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
pub type ApiError = Custom<Json<ErrorResponse>>;
pub type ApiResult<T> = Result<Json<T>, ApiError>;
pub fn err(status: Status, error: &str) -> ApiError {
    Custom(
        status,
        Json(ErrorResponse {
            error: error.into(),
        }),
    )
}
pub fn invalid(_: impl std::fmt::Display) -> ApiError {
    err(Status::BadRequest, "invalid_request")
}
pub fn denied(_: impl std::fmt::Display) -> ApiError {
    err(Status::Forbidden, "authorization_denied")
}
pub fn internal(e: impl std::fmt::Display) -> ApiError {
    // Never include query parameters, proof material, action logs or upstream bodies.
    let _ = e;
    tracing::warn!("keychain storage operation failed");
    err(Status::InternalServerError, "storage_unavailable")
}
