//! JSON error catchers for the API.
//!
//! Rocket's default catchers render `text/html` error pages. For a JSON API
//! this breaks every client that parses responses as JSON (including the
//! README quickstart, which pipes `curl` into `jq`). Every catcher here
//! returns a machine-readable body:
//!
//! ```json
//! {
//!   "error": "payment_required",
//!   "message": "what went wrong",
//!   "fix": "what to do about it",
//!   "docs_url": "where to read more"
//! }
//! ```
//!
//! Guards can attach a request-scoped [`ErrorDetail`] (via
//! [`set_error_detail`]) so the catcher explains *why* the request failed
//! (e.g. "key not recognized" vs "insufficient credits") instead of a
//! generic per-status message.

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{Request, catch};
use serde::Serialize;

pub const DASHBOARD_URL: &str = "https://dashboard.chipotle.litprotocol.com/dapps/dashboard/";
pub const DOCS_ERRORS_URL: &str = "https://developer.litprotocol.com/management/errors";
pub const DOCS_AUTH_URL: &str = "https://developer.litprotocol.com/architecture/authModel";
pub const DOCS_PRICING_URL: &str = "https://developer.litprotocol.com/management/pricing";

/// JSON body returned by every error catcher.
#[derive(Serialize, Debug, Clone)]
pub struct ApiError {
    /// Stable machine-readable identifier (snake_case of the status reason).
    pub error: &'static str,
    /// Human-readable explanation of what went wrong.
    pub message: String,
    /// Actionable next step.
    pub fix: String,
    /// Where to read more.
    pub docs_url: &'static str,
}

/// Request-local error context, set by guards before they fail so the
/// catcher can produce a specific message instead of the generic one.
#[derive(Default, Clone)]
pub struct ErrorDetail {
    pub message: Option<String>,
    pub fix: Option<String>,
}

/// Attach a specific message/fix to the request for the catcher to pick up.
///
/// Must be called before the guard returns its `Outcome::Error` — request
/// local cache is write-once per type, so the first caller wins.
pub fn set_error_detail(request: &Request<'_>, message: impl Into<String>, fix: impl Into<String>) {
    request.local_cache(|| ErrorDetail {
        message: Some(message.into()),
        fix: Some(fix.into()),
    });
}

fn detail(req: &Request<'_>) -> ErrorDetail {
    req.local_cache(ErrorDetail::default).clone()
}

fn api_error(
    req: &Request<'_>,
    error: &'static str,
    default_message: &str,
    default_fix: String,
    docs_url: &'static str,
) -> Json<ApiError> {
    let d = detail(req);
    Json(ApiError {
        error,
        message: d.message.unwrap_or_else(|| default_message.to_string()),
        fix: d.fix.unwrap_or(default_fix),
        docs_url,
    })
}

#[catch(400)]
pub fn bad_request(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "bad_request",
        "The request could not be understood — usually a malformed JSON body or a missing required field.",
        "Compare your request against the OpenAPI schema at /core/v1/swagger-ui (raw spec: /core/v1/openapi.json).".to_string(),
        DOCS_ERRORS_URL,
    )
}

#[catch(401)]
pub fn unauthorized(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "unauthorized",
        "Missing or invalid API key.",
        "Send your key in the X-Api-Key header (or Authorization: Bearer <key>). \
         New here? Create an account with POST /core/v1/new_account."
            .to_string(),
        DOCS_AUTH_URL,
    )
}

#[catch(402)]
pub fn payment_required(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "payment_required",
        "Your account does not have enough credits for this operation.",
        format!(
            "Add funds (minimum $5.00, card or crypto) in the dashboard at {DASHBOARD_URL} \
             or via POST /core/v1/billing/create_payment_intent. \
             Check your balance with GET /core/v1/billing/balance."
        ),
        DOCS_PRICING_URL,
    )
}

#[catch(403)]
pub fn forbidden(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "forbidden",
        "This API key is not permitted to perform this operation.",
        "Check the key's scopes with GET /core/v1/list_api_keys, or retry with your account \
         master key. Usage keys only act in the groups they were granted."
            .to_string(),
        DOCS_AUTH_URL,
    )
}

#[catch(404)]
pub fn not_found(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "not_found",
        "No such endpoint.",
        "Browse the API at /core/v1/swagger-ui or fetch the spec at /core/v1/openapi.json."
            .to_string(),
        DOCS_ERRORS_URL,
    )
}

#[catch(422)]
pub fn unprocessable_entity(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "unprocessable_entity",
        "The JSON body does not match the expected schema for this endpoint.",
        "Compare your request against the OpenAPI schema at /core/v1/swagger-ui (raw spec: /core/v1/openapi.json).".to_string(),
        DOCS_ERRORS_URL,
    )
}

#[catch(429)]
pub fn too_many_requests(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "too_many_requests",
        "The node is shedding load (CPU overloaded).",
        "Retry with backoff. Spread sustained load over time or across requests.".to_string(),
        DOCS_ERRORS_URL,
    )
}

#[catch(500)]
pub fn internal_error(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "internal_error",
        "Something went wrong on our side.",
        "Retry the request. If the problem persists, contact us — links at \
         https://developer.litprotocol.com/."
            .to_string(),
        DOCS_ERRORS_URL,
    )
}

#[catch(503)]
pub fn service_unavailable(req: &Request<'_>) -> Json<ApiError> {
    api_error(
        req,
        "service_unavailable",
        "A dependency (billing, chain RPC, or the actions runtime) is temporarily unavailable.",
        "Retry in a few seconds. Nothing was charged.".to_string(),
        DOCS_ERRORS_URL,
    )
}

#[catch(default)]
pub fn default_catcher(status: Status, req: &Request<'_>) -> Json<ApiError> {
    let d = detail(req);
    Json(ApiError {
        error: "error",
        message: d
            .message
            .unwrap_or_else(|| format!("{} {}.", status.code, status.reason_lossy())),
        fix: d.fix.unwrap_or_else(|| {
            "See the errors reference for what this status means on this API.".to_string()
        }),
        docs_url: DOCS_ERRORS_URL,
    })
}

/// All catchers, for registration in `main.rs`.
pub fn catchers() -> Vec<rocket::Catcher> {
    rocket::catchers![
        bad_request,
        unauthorized,
        payment_required,
        forbidden,
        not_found,
        unprocessable_entity,
        too_many_requests,
        internal_error,
        service_unavailable,
        default_catcher,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use rocket::request::{FromRequest, Outcome, Request};
    use rocket::{get, routes};

    /// Guard that always fails with 402 after attaching a specific detail,
    /// mimicking the billing guard's behaviour.
    struct AlwaysBroke;

    #[rocket::async_trait]
    impl<'r> FromRequest<'r> for AlwaysBroke {
        type Error = ();
        async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
            set_error_detail(
                request,
                "Insufficient credits: this call needs $0.01 but your balance is $0.00.",
                "Top up at the dashboard.",
            );
            Outcome::Error((Status::PaymentRequired, ()))
        }
    }

    #[get("/guarded")]
    fn guarded(_g: AlwaysBroke) -> &'static str {
        "unreachable"
    }

    #[get("/fails_plain")]
    fn fails_plain() -> Result<&'static str, Status> {
        Err(Status::PaymentRequired)
    }

    fn client() -> Client {
        let rocket = rocket::build()
            .mount("/", routes![guarded, fails_plain])
            .register("/", catchers());
        Client::tracked(rocket).expect("valid rocket")
    }

    #[test]
    fn catcher_returns_json_with_status_preserved() {
        let client = client();
        let resp = client.get("/fails_plain").dispatch();
        assert_eq!(resp.status(), Status::PaymentRequired);
        assert_eq!(resp.content_type(), Some(ContentType::JSON));
        let body = resp.into_string().expect("body");
        assert!(body.contains("payment_required"), "body: {body}");
        assert!(body.contains(DASHBOARD_URL), "body: {body}");
    }

    #[test]
    fn guard_detail_overrides_generic_message() {
        let client = client();
        let resp = client.get("/guarded").dispatch();
        assert_eq!(resp.status(), Status::PaymentRequired);
        let body = resp.into_string().expect("body");
        assert!(
            body.contains("your balance is $0.00"),
            "guard-provided detail should appear in catcher body: {body}"
        );
    }

    #[test]
    fn unknown_path_is_json_404() {
        let client = client();
        let resp = client.get("/definitely_not_a_route").dispatch();
        assert_eq!(resp.status(), Status::NotFound);
        assert_eq!(resp.content_type(), Some(ContentType::JSON));
        let body = resp.into_string().expect("body");
        assert!(body.contains("not_found"), "body: {body}");
        assert!(body.contains("swagger-ui"), "body: {body}");
    }

    #[test]
    fn default_catcher_handles_unlisted_status() {
        // 405 Method Not Allowed has no dedicated catcher — POST to a GET route.
        let client = client();
        let resp = client.post("/fails_plain").dispatch();
        // Rocket reports 404 or 405 depending on routing internals; either way
        // the body must be JSON from one of our catchers.
        assert_eq!(resp.content_type(), Some(ContentType::JSON));
    }
}
