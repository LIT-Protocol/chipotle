use anyhow::Result;
use rocket::http::Status;
use rocket_okapi::{
    OpenApiError, r#gen::OpenApiGenerator, okapi::schemars::JsonSchema,
    response::OpenApiResponderInner,
};
use rocket_responder::{
    ApiResponse, bad_request, forbidden, internal_server_error, not_found, ok, payment_required,
    unauthorized,
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use tracing::{error, warn};
// This is the endpoint response error message
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct ErrMessage(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiStatus {
    #[schemars(with = "String")]
    pub status: Status,
    pub message: String,
}

impl Display for ApiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<ApiStatus> for ErrMessage {
    fn from(status: ApiStatus) -> Self {
        ErrMessage(status.message)
    }
}

impl From<Status> for ApiStatus {
    fn from(status: Status) -> Self {
        Self {
            status,
            message: status.reason_lossy().to_string(),
        }
    }
}

impl From<hex::FromHexError> for ApiStatus {
    fn from(e: hex::FromHexError) -> Self {
        Self::bad_request(e.into(), "Invalid hex string.")
    }
}

impl From<lit_core::error::Error> for ApiStatus {
    fn from(e: lit_core::error::Error) -> Self {
        Self::internal_server_error(e.into(), "Internal server error.")
    }
}

impl From<anyhow::Error> for ApiStatus {
    fn from(e: anyhow::Error) -> Self {
        Self::internal_server_error(e, "Internal server error.")
    }
}

impl From<std::num::ParseFloatError> for ApiStatus {
    fn from(e: std::num::ParseFloatError) -> Self {
        Self::bad_request(e.into(), "Invalid float string.")
    }
}

impl From<serde_bare::Error> for ApiStatus {
    fn from(e: serde_bare::Error) -> Self {
        Self::internal_server_error(e.into(), "Serde bare error.")
    }
}

impl From<ethers::abi::ethereum_types::FromStrRadixErr> for ApiStatus {
    fn from(e: ethers::abi::ethereum_types::FromStrRadixErr) -> Self {
        Self::bad_request(e.into(), "Invalid radix string.")
    }
}

impl From<url::ParseError> for ApiStatus {
    fn from(e: url::ParseError) -> Self {
        Self::bad_request(e.into(), "Invalid URL.")
    }
}

impl From<ethers::providers::ProviderError> for ApiStatus {
    fn from(e: ethers::providers::ProviderError) -> Self {
        Self::internal_server_error(e.into(), "Provider error.")
    }
}

impl From<ethers::utils::ConversionError> for ApiStatus {
    fn from(e: ethers::utils::ConversionError) -> Self {
        Self::internal_server_error(e.into(), "Conversion error.")
    }
}

// Wrapper type to implement From trait for Result<T, ApiStatus>
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ApiResult<T>(pub Result<T, ApiStatus>);

impl<T: Serialize> From<ApiResult<T>> for ApiResponse<T, ErrMessage> {
    fn from(api_result: ApiResult<T>) -> Self {
        match api_result.0 {
            Ok(response) => ok(response),
            Err(status) => match status.status.code {
                500 => internal_server_error(ErrMessage(status.message)),
                400 => bad_request(ErrMessage(status.message)),
                404 => not_found(ErrMessage(status.message)),
                401 => unauthorized(ErrMessage(status.message)),
                403 => forbidden(ErrMessage(status.message)),
                402 => payment_required(ErrMessage(status.message)),
                _ => internal_server_error(ErrMessage(format!(
                    "Unhandled error code #{}: {}",
                    status.status.code,
                    status.status.reason_lossy()
                ))),
            },
        }
    }
}

impl ApiStatus {
    pub fn add_message(&mut self, message: impl Into<String>) -> Self {
        self.message = format!("{}: {}", self.message, message.into());
        self.clone()
    }

    // these are helper functions...
    pub fn internal_server_error(e: anyhow::Error, message: impl Into<String>) -> Self {
        let message = format!("{}: {:?}", message.into(), e);
        error!("internal_server_error: {:?}", message);
        Self {
            status: Status::InternalServerError,
            message,
        }
    }

    pub fn bad_request(e: anyhow::Error, message: impl Into<String>) -> Self {
        let message = format!("{}: {:?}", message.into(), e);
        warn!("bad_request: {:?}", message);
        Self {
            status: Status::BadRequest,
            message,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        let message = message.into();
        warn!("not_found: {:?}", message);
        Self {
            status: Status::NotFound,
            message,
        }
    }

    pub fn payment_required(message: impl Into<String>) -> Self {
        let message = message.into();
        warn!("payment_required: {:?}", message);
        Self {
            status: Status::PaymentRequired,
            message,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        let message = message.into();
        warn!("Unauthorized: {:?}", message);
        Self {
            status: Status::Unauthorized,
            message,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        let message = message.into();
        warn!("Forbidden: {:?}", message);
        Self {
            status: Status::Forbidden,
            message,
        }
    }
    pub fn option_not_found(message: impl Into<String>) -> Self {
        let message = message.into();
        warn!("Option not found: {:?}", message);
        Self {
            status: Status::InternalServerError,
            message,
        }
    }
}

impl OpenApiResponderInner for ApiStatus {
    fn responses(
        generator: &mut OpenApiGenerator,
    ) -> std::result::Result<rocket_okapi::okapi::openapi3::Responses, OpenApiError> {
        let mut responses = rocket_okapi::okapi::openapi3::Responses::default();
        let schema = generator.json_schema::<ApiStatus>();
        rocket_okapi::util::add_default_response_schema(
            &mut responses,
            "application/json".to_string(),
            schema,
        );
        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Status code constructors ────────────────────────────────────────
    #[test]
    fn bad_request_has_400() {
        let status = ApiStatus::bad_request(anyhow::anyhow!("test"), "msg");
        assert_eq!(status.status, Status::BadRequest);
        assert!(status.message.contains("msg"));
    }

    #[test]
    fn internal_server_error_has_500() {
        let status = ApiStatus::internal_server_error(anyhow::anyhow!("boom"), "msg");
        assert_eq!(status.status, Status::InternalServerError);
    }

    #[test]
    fn not_found_has_404() {
        let status = ApiStatus::not_found("gone");
        assert_eq!(status.status, Status::NotFound);
        assert_eq!(status.message, "gone");
    }

    #[test]
    fn unauthorized_has_401() {
        let status = ApiStatus::unauthorized("denied");
        assert_eq!(status.status, Status::Unauthorized);
        assert_eq!(status.message, "denied");
    }

    #[test]
    fn forbidden_has_403() {
        let status = ApiStatus::forbidden("nope");
        assert_eq!(status.status, Status::Forbidden);
        assert_eq!(status.message, "nope");
    }

    #[test]
    fn payment_required_has_402() {
        let status = ApiStatus::payment_required("pay up");
        assert_eq!(status.status, Status::PaymentRequired);
        assert_eq!(status.message, "pay up");
    }

    #[test]
    fn option_not_found_uses_500() {
        let status = ApiStatus::option_not_found("missing");
        assert_eq!(status.status, Status::InternalServerError);
        assert_eq!(status.message, "missing");
    }

    // ── add_message ────────────────────────────────────────────────────
    #[test]
    fn add_message_appends() {
        let mut status = ApiStatus::not_found("base");
        status.add_message("extra");
        assert_eq!(status.message, "base: extra");
    }

    // ── Display ────────────────────────────────────────────────────────
    #[test]
    fn display_shows_message() {
        let status = ApiStatus::not_found("hello");
        assert_eq!(format!("{}", status), "hello");
    }

    // ── From<Status> ───────────────────────────────────────────────────
    #[test]
    fn from_rocket_status() {
        let api_status: ApiStatus = Status::NotFound.into();
        assert_eq!(api_status.status, Status::NotFound);
        assert!(!api_status.message.is_empty());
    }

    // ── From<hex::FromHexError> ─────────────────────────────────────────
    #[test]
    fn from_hex_error() {
        let hex_err = hex::decode("zz").unwrap_err();
        let api_status: ApiStatus = hex_err.into();
        assert_eq!(api_status.status, Status::BadRequest);
    }

    // ── From<anyhow::Error> ─────────────────────────────────────────────
    #[test]
    fn from_anyhow_error() {
        let err = anyhow::anyhow!("something broke");
        let api_status: ApiStatus = err.into();
        assert_eq!(api_status.status, Status::InternalServerError);
    }

    // ── From<ParseFloatError> ──────────────────────────────────────────
    #[test]
    fn from_parse_float_error() {
        let err = "abc".parse::<f64>().unwrap_err();
        let api_status: ApiStatus = err.into();
        assert_eq!(api_status.status, Status::BadRequest);
    }

    // ── ErrMessage conversion ──────────────────────────────────────────
    #[test]
    fn into_err_message() {
        let status = ApiStatus::not_found("test message");
        let err_msg: ErrMessage = status.into();
        assert_eq!(err_msg.0, "test message");
    }

    // ── Serde round-trip ───────────────────────────────────────────────
    #[test]
    fn serde_round_trip() {
        let status = ApiStatus::not_found("test");
        let json = serde_json::to_string(&status).unwrap();
        let back: ApiStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message, "test");
    }
}
