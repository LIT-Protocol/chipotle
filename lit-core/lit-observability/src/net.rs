#[cfg(feature = "otlp")]
use lit_core::{config::LitConfig, error::Result};
#[cfg(feature = "otlp")]
use opentelemetry_otlp::{TonicExporterBuilder, WithExportConfig};

#[cfg(feature = "otlp")]
const DEFAULT_EXPORTER_ENDPOINT: &str = "http://127.0.0.1:4317";

#[cfg(feature = "otlp")]
pub(crate) fn init_tonic_exporter_builder(cfg: &LitConfig) -> Result<TonicExporterBuilder> {
    let endpoint = cfg.get_string("telemetry.endpoint").unwrap_or_else(|_| DEFAULT_EXPORTER_ENDPOINT.to_string());
    
    Ok(opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint))
}

pub mod grpc {
    pub use tonic_middleware;
    use tracing::Instrument;
    use tracing::info_span;
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
                .get("x-correlation-id")
                .or_else(|| req.headers().get("x-request-id"))
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty());

            let info_span = match correlation_id {
                Some(id) => info_span!(
                    "handle_grpc_request",
                    method = %req.method(),
                    path = %req.uri().path(),
                    correlation_id = %id,
                ),
                None => info_span!(
                    "handle_grpc_request",
                    method = %req.method(),
                    path = %req.uri().path(),
                ),
            };

            #[cfg(feature = "otlp")]
            info_span.set_parent(parent_cx);

            service.call(req).instrument(info_span).await
        }
    }
}
