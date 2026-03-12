#[cfg(feature = "otlp")]
use lit_core::error::Result;
#[cfg(feature = "otlp")]
use opentelemetry_otlp::{TonicExporterBuilder, WithExportConfig};

#[cfg(feature = "otlp")]
pub(crate) fn init_tonic_exporter_builder(endpoint: &str) -> Result<TonicExporterBuilder> {
    Ok(opentelemetry_otlp::new_exporter().tonic().with_endpoint(endpoint))
}

pub mod grpc {
    pub use tonic_middleware;
    use tonic::codegen::http::header::{HeaderName, HeaderValue};
    use tracing::Instrument;
    use tracing::info_span;
    use uuid::Uuid;
    #[cfg(feature = "otlp")]
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    #[cfg(feature = "otlp")]
    use opentelemetry::global;
    use tonic::async_trait;
    use tonic::body::BoxBody;
    use tonic::codegen::http::Request as HttpRequest;
    use tonic::codegen::http::Response as HttpResponse;
    use tonic_middleware::Middleware;
    use tonic_middleware::ServiceBound;

    #[cfg(feature = "otlp")]
    use crate::tracing::propagation::HttpMetadataMap;

    const HEADER_X_CORRELATION_ID: &str = "x-correlation-id";
    const HEADER_X_REQUEST_ID: &str = "x-request-id";

    /// TracingMiddleware is a middleware that handles tracing context that is propagated across process boundaries.
    #[derive(Clone)]
    pub struct TracingMiddleware;

    #[async_trait]
    impl<S> Middleware<S> for TracingMiddleware
    where
        S: ServiceBound,
        S::Future: Send,
    {
        #[allow(unused_mut)]
        async fn call(
            &self, mut req: HttpRequest<BoxBody>, mut service: S,
        ) -> Result<HttpResponse<BoxBody>, S::Error> {
            #[cfg(feature = "otlp")]
            let parent_cx = global::get_text_map_propagator(|propagator| {
                propagator.extract(&HttpMetadataMap(req.headers_mut()))
            });

            let correlation_id = req
                .headers()
                .get(HEADER_X_CORRELATION_ID)
                .or_else(|| req.headers().get(HEADER_X_REQUEST_ID))
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let request_id = req
                .headers()
                .get(HEADER_X_REQUEST_ID)
                .or_else(|| req.headers().get(HEADER_X_CORRELATION_ID))
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| correlation_id.clone());

            let info_span = info_span!(
                "handle_grpc_request",
                method = %req.method(),
                path = %req.uri().path(),
                correlation_id = %correlation_id,
                request_id = %request_id,
            );

            #[cfg(feature = "otlp")]
            info_span.set_parent(parent_cx);

            let mut response = service.call(req).instrument(info_span).await?;

            if let (Ok(name_correlation), Ok(value_correlation)) = (
                HeaderName::try_from(HEADER_X_CORRELATION_ID),
                HeaderValue::try_from(correlation_id.as_str()),
            ) {
                response.headers_mut().insert(name_correlation, value_correlation);
            }
            if let (Ok(name_request), Ok(value_request)) = (
                HeaderName::try_from(HEADER_X_REQUEST_ID),
                HeaderValue::try_from(request_id.as_str()),
            ) {
                response.headers_mut().insert(name_request, value_request);
            }

            Ok(response)
        }
    }
}
