use anyhow::Result;
use lit_rust_crypto::blsful::BlsError;
use rocket::http::Status;
use rocket_okapi::{
    OpenApiError, r#gen::OpenApiGenerator, okapi::schemars::JsonSchema,
    response::OpenApiResponderInner,
};
use rocket_responder::{
    ApiResponse, bad_request, internal_server_error, not_found, ok, payment_required,
};
use serde::{Deserialize, Serialize};
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

impl From<BlsError> for ApiStatus {
    fn from(e: BlsError) -> Self {
        Self::internal_server_error(e.into(), "Bls error.")
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
