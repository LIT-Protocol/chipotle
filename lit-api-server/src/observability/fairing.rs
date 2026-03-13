//! Observability fairing for lit-api-server.
//!
//! Creates a request span, extracts or generates correlation/request IDs,
//! propagates context to logs and OTel, and adds X-Correlation-Id and
//! X-Request-Id to all responses (success and error).

use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::{Request, Response};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use tracing::{field, info_span};
use uuid::Uuid;

const HEADER_X_CORRELATION_ID: &str = "X-Correlation-Id";
const HEADER_X_PRIVACY_MODE: &str = "X-Privacy-Mode";
const HEADER_X_REQUEST_ID: &str = "X-Request-Id";

/// Context stored per request for response header injection and span propagation.
/// The root span is stored as a non-entered `tracing::Span` (which is Send + Sync)
/// because `EnteredSpan` is !Send and cannot live in Rocket's `local_cache`.
/// The span stays alive for the full request lifecycle because `local_cache` is
/// dropped only after the response is sent.
struct RequestTracingContext {
    /// User-provided X-Correlation-Id, if any. Only set when user explicitly sent the header.
    /// Per requirements: X-Correlation-Id MUST NOT be on response if user did not provide it.
    user_provided_correlation_id: Option<String>,
    /// Span/request ID. Always present. Propagated as X-Request-Id on response.
    request_id: String,
    /// Root span covering the full request lifecycle. Closed when local_cache is dropped
    /// (after response is sent), satisfying the requirement that the root span covers
    /// both success and failure responses.
    span: tracing::Span,
}

impl RequestTracingContext {
    fn dummy() -> Self {
        Self {
            user_provided_correlation_id: None,
            request_id: String::new(),
            span: tracing::Span::none(),
        }
    }
}

#[derive(Clone)]
pub struct ObservabilityFairing;

impl ObservabilityFairing {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ObservabilityFairing {
    fn default() -> Self {
        Self::new()
    }
}

/// Request guard that provides the correlation ID from the ObservabilityFairing's cache.
/// Use this in routes that need to propagate the correlation ID to downstream services (e.g. lit-actions).
#[derive(Clone, Debug)]
pub struct CorrelationId(pub Option<String>);

#[rocket::async_trait]
impl<'r> rocket::request::FromRequest<'r> for CorrelationId {
    type Error = std::convert::Infallible;

    async fn from_request(req: &'r Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let ctx = req.local_cache(RequestTracingContext::dummy);
        rocket::request::Outcome::Success(CorrelationId(ctx.user_provided_correlation_id.clone()))
    }
}

impl<'r> OpenApiFromRequest<'r> for CorrelationId {
    fn from_request_input(
        generator: &mut OpenApiGenerator,
        _name: String,
        required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        let schema = generator.json_schema::<String>();
        Ok(RequestHeaderInput::Parameter(Parameter {
            name: HEADER_X_CORRELATION_ID.to_owned(),
            location: "header".to_owned(),
            description: Some(
                "Correlation ID for request tracing. Auto-generated if not provided.".to_owned(),
            ),
            required,
            deprecated: false,
            allow_empty_value: false,
            value: ParameterValue::Schema {
                style: None,
                explode: None,
                allow_reserved: false,
                schema,
                example: None,
                examples: None,
            },
            extensions: Object::default(),
        }))
    }
}

/// Request guard providing the root span and request/correlation IDs.
/// Handlers that take this guard can use `.instrument(span)` or `#[instrument(parent = span)]`
/// to make their work appear as child spans of the root HTTP request span.
#[derive(Clone, Debug)]
pub struct RequestSpan {
    pub span: tracing::Span,
    pub request_id: String,
    pub correlation_id: Option<String>,
}

#[rocket::async_trait]
impl<'r> rocket::request::FromRequest<'r> for RequestSpan {
    type Error = std::convert::Infallible;

    async fn from_request(req: &'r Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let ctx = req.local_cache(RequestTracingContext::dummy);
        rocket::request::Outcome::Success(RequestSpan {
            span: ctx.span.clone(),
            request_id: ctx.request_id.clone(),
            correlation_id: ctx.user_provided_correlation_id.clone(),
        })
    }
}

impl<'r> OpenApiFromRequest<'r> for RequestSpan {
    fn from_request_input(
        _generator: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        // Internal guard — no user-visible header input.
        Ok(RequestHeaderInput::None)
    }
}

#[rocket::async_trait]
impl Fairing for ObservabilityFairing {
    fn info(&self) -> Info {
        Info {
            name: "Observability Fairing",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, req: &mut Request<'_>, _data: &mut rocket::Data<'_>) {
        let user_provided_correlation_id = req
            .headers()
            .get_one(HEADER_X_CORRELATION_ID)
            .map(String::from)
            .filter(|s| !s.is_empty());

        // X-Request-Id must NOT be provided by the user. Always generate a new random UUID.
        // Per requirements: if user sends X-Request-Id, it must be ignored.
        let privacy_mode = req.headers().get_one(HEADER_X_PRIVACY_MODE).is_some();
        let request_id = if privacy_mode {
            format!("{}_{}", Uuid::new_v4(), lit_observability::PRIVACY_MODE_TAG)
        } else {
            Uuid::new_v4().to_string()
        };

        let span = info_span!(
            "http_request",
            method = %req.method(),
            uri = %req.uri(),
            correlation_id = field::Empty,
            request_id = %request_id,
            http.status_code = field::Empty,
        );

        if let Some(ref cid) = user_provided_correlation_id {
            span.record("correlation_id", cid.as_str());
        }

        // Enter the span briefly to set task-local request context for log correlation.
        span.in_scope(|| {
            lit_observability::logging::set_request_context(
                Some(request_id.clone()),
                user_provided_correlation_id.clone(),
            );
        });

        let ctx = RequestTracingContext {
            user_provided_correlation_id,
            request_id,
            span,
        };
        let _ = req.local_cache(|| ctx);
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        let ctx = req.local_cache(RequestTracingContext::dummy);

        // Record the response status on the root span so traces include the outcome.
        ctx.span.record("http.status_code", res.status().code);

        if let Some(ref id) = ctx.user_provided_correlation_id {
            res.set_header(Header::new(HEADER_X_CORRELATION_ID, id.clone()));
        }
        if !ctx.request_id.is_empty() {
            res.set_header(Header::new(HEADER_X_REQUEST_ID, ctx.request_id.clone()));
        }
        // Clear task-local context to prevent stale entries if tokio reuses this task ID.
        ctx.span.in_scope(|| {
            lit_observability::logging::clear_task_request_context();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Status;
    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};

    #[get("/test-span")]
    fn test_route() -> &'static str {
        "ok"
    }

    /// Route that always returns 500 for testing failure response headers.
    #[get("/test-error")]
    fn test_error_route() -> Status {
        Status::InternalServerError
    }

    fn test_rocket() -> rocket::Rocket<rocket::Build> {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .with_test_writer()
            .try_init();
        rocket::build()
            .attach(ObservabilityFairing::new())
            .mount("/", routes![test_route, test_error_route])
    }

    /// Data requirement: Each API request must be end-to-end traceable. A span with a random ID
    /// is created when a request arrives and propagated to the response.
    #[tokio::test]
    async fn span_id_created_and_propagated_end_to_end() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let response = client.get("/test-span").dispatch().await;
        assert_eq!(response.status().code, 200);

        let request_id = response
            .headers()
            .get_one(HEADER_X_REQUEST_ID)
            .expect("X-Request-Id must be present for end-to-end traceability");

        let parsed = Uuid::parse_str(request_id).expect("span ID must be valid UUID");
        assert_eq!(
            parsed.get_version(),
            Some(uuid::Version::Random),
            "span ID must be UUID v4 (random) for support traceability"
        );
    }

    /// Data requirement: X-Request-Id MUST be attached to both success and failure responses.
    #[tokio::test]
    async fn x_request_id_on_success_response() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let response = client.get("/test-span").dispatch().await;
        assert_eq!(response.status().code, 200);

        assert!(
            response.headers().get_one(HEADER_X_REQUEST_ID).is_some(),
            "X-Request-Id MUST be attached to success response"
        );
    }

    /// Data requirement: X-Request-Id MUST be attached to both success and failure responses.
    #[tokio::test]
    async fn x_request_id_on_failure_response() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let response = client.get("/test-error").dispatch().await;
        assert_eq!(response.status().code, 500);

        assert!(
            response.headers().get_one(HEADER_X_REQUEST_ID).is_some(),
            "X-Request-Id MUST be attached to failure response"
        );
    }

    /// Data requirement: X-Request-Id MUST be on 404 (catcher) responses.
    #[tokio::test]
    async fn x_request_id_on_404_response() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let response = client.get("/nonexistent-path").dispatch().await;
        assert_eq!(response.status().code, 404);

        assert!(
            response.headers().get_one(HEADER_X_REQUEST_ID).is_some(),
            "X-Request-Id MUST be attached to 404 response"
        );
    }

    /// Data requirement: When user provides X-Correlation-Id, it must be propagated to the response.
    #[tokio::test]
    async fn x_correlation_id_propagated_when_user_provides_it() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let user_correlation_id = "user-correlation-abc-123";
        let response = client
            .get("/test-span")
            .header(rocket::http::Header::new(
                HEADER_X_CORRELATION_ID,
                user_correlation_id,
            ))
            .dispatch()
            .await;

        assert_eq!(response.status().code, 200);

        let returned = response
            .headers()
            .get_one(HEADER_X_CORRELATION_ID)
            .expect("X-Correlation-Id MUST be returned when user provides it");

        assert_eq!(
            returned, user_correlation_id,
            "X-Correlation-Id must be propagated from request to response"
        );
    }

    /// Data requirement: X-Correlation-Id MUST NOT be on response when user did not provide it.
    #[tokio::test]
    async fn x_correlation_id_absent_when_user_does_not_provide_it() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let response = client.get("/test-span").dispatch().await;
        assert_eq!(response.status().code, 200);

        let correlation_id = response.headers().get_one(HEADER_X_CORRELATION_ID);
        assert!(
            correlation_id.is_none(),
            "X-Correlation-Id MUST NOT be attached when user did not provide it in the request"
        );
    }

    /// Data requirement: X-Correlation-Id MUST NOT be on response when header was missing.
    #[tokio::test]
    async fn x_correlation_id_absent_when_header_missing() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        // No X-Correlation-Id header at all
        let response = client
            .get("/test-span")
            .header(rocket::http::Header::new("Some-Other-Header", "value"))
            .dispatch()
            .await;

        assert_eq!(response.status().code, 200);

        assert!(
            response
                .headers()
                .get_one(HEADER_X_CORRELATION_ID)
                .is_none(),
            "X-Correlation-Id MUST NOT be attached when header was missing in request"
        );
    }

    /// Data requirement: X-Request-Id must NOT be provided by the user. If the user sends
    /// X-Request-Id, it must be ignored and a new random UUID created.
    #[tokio::test]
    async fn user_provided_x_request_id_ignored_new_uuid_created() {
        let client = Client::tracked(test_rocket()).await.expect("valid rocket");

        let user_sent_request_id = "550e8400-e29b-41d4-a716-446655440000";
        let response = client
            .get("/test-span")
            .header(rocket::http::Header::new(
                HEADER_X_REQUEST_ID,
                user_sent_request_id,
            ))
            .dispatch()
            .await;

        assert_eq!(response.status().code, 200);

        let returned = response
            .headers()
            .get_one(HEADER_X_REQUEST_ID)
            .expect("X-Request-Id must be present");

        assert_ne!(
            returned, user_sent_request_id,
            "User-provided X-Request-Id must be ignored; server must create a new UUID"
        );

        let parsed = Uuid::parse_str(returned).expect("returned request ID must be valid UUID");
        assert_eq!(
            parsed.get_version(),
            Some(uuid::Version::Random),
            "returned X-Request-Id must be a new random UUID v4"
        );
    }
}
