use http_body_util::Full;
use hyper::{Response, StatusCode, body::Bytes};
use serde_json::json;
use tracing::error;

use crate::error::{Error, err_to_public_error};

pub trait ApiError {
    fn handle(self) -> Response<Full<Bytes>>;
    fn handle_with_details(self, details: String) -> Response<Full<Bytes>>;
}

impl ApiError for Error {
    fn handle(self) -> Response<Full<Bytes>> {
        handle_err(self)
    }

    fn handle_with_details(self, details: String) -> Response<Full<Bytes>> {
        handle_err_with_details(self, Some(details))
    }
}

pub fn handle_err(err: Error) -> Response<Full<Bytes>> {
    handle_err_with_details(err, None)
}

pub fn handle_err_with_details(err: Error, details: Option<String>) -> Response<Full<Bytes>> {
    error!(error = ?err, "Handling API error");

    let public = err_to_public_error(err, details);
    let status = StatusCode::from_u16(public.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let public_json = public.to_json();
    match public_json {
        Ok(json) => {
            let err_resp =
                Response::builder().status(status).body(Full::new(Bytes::from(json.to_string())));
            if let Ok(resp) = err_resp {
                return resp;
            }
        }
        Err(e) => error!(error = ?e, "Failed to serialize error to json"),
    }

    // For cases where the above fails to serialize.
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from(
            json!({
                "errorKind": public.error_kind(),
                "message": public.message().unwrap(),
            })
            .to_string(),
        )))
        .expect("Unable to build default error response. This should never occur.")
    // This should never panic.
}
