use std::time::Duration;

use hyper_util::rt::TokioIo;
use lit_core::{config::LitConfig, error::Result};
use lit_logging::config::{LitLoggingConfig, logging_service_grpc_socket_path};
use opentelemetry_otlp::{ExportConfig, TonicExporterBuilder, WithExportConfig};
use tokio::net::UnixStream;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

use crate::error::unexpected_err;

const DEFAULT_EXPORTER_ENDPOINT: &str = "http://127.0.0.1:4371";

pub(crate) async fn init_tonic_exporter_builder(
    cfg: &LitConfig, #[cfg(feature = "proxy-collector")] proxy_collector_name: &'static str,
) -> Result<TonicExporterBuilder> {
    // If Jaeger logging is enabled, we send logs to a local GRPC server.
    if cfg.logging_jaeger() {
        println!("Exporting telemetry to default tonic endpoint");
        return Ok(opentelemetry_otlp::new_exporter().tonic());
    }

    // Tonic will ignore this uri because uds do not use it
    // if the connector does use the uri it will be provided
    // as the request to the `MakeConnection`.
    let channel = Endpoint::try_from(DEFAULT_EXPORTER_ENDPOINT)
        .map_err(|e| unexpected_err(e.to_string(), Some("Could not create endpoint".to_string())))?
        .connect_with_connector(service_fn(|_: Uri| async {
            // Try connecting up to 5 times
            for _ in 0..5 {
                match UnixStream::connect(logging_service_grpc_socket_path()).await {
                    Ok(stream) => return Ok(TokioIo::new(stream)),
                    Err(e) => {
                        println!("Failed to connect to UDS socket: {}, retrying up to 5 times.", e);

                        // Wait for 5 seconds before retrying.
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }

            Err(unexpected_err("Failed to connect to UDS socket after retrying", None))
        }))
        .await
        .map_err(|e| {
            unexpected_err(e.to_string(), Some("Could not connect to logging service".to_string()))
        })?;

    // First, create a OTLP exporter builder. Configure it as you need.
    #[allow(unused_mut)]
    let mut exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_channel(channel)
        .with_export_config(ExportConfig {
            endpoint: DEFAULT_EXPORTER_ENDPOINT.to_string(),
            protocol: opentelemetry_otlp::Protocol::Grpc,
            timeout: Duration::from_secs(3),
        });

    #[cfg(feature = "proxy-collector")]
    {
        exporter = exporter.with_interceptor(
            |mut req: tonic::Request<()>| -> core::result::Result<tonic::Request<()>, tonic::Status> {
                req.metadata_mut().insert(grpc::EXPORT_ORIGIN_HEADER, tonic::metadata::MetadataValue::from_static(proxy_collector_name));
                Ok(req)
            },
        );
    }

    Ok(exporter)
}

pub mod grpc {
    pub use tonic_middleware;
    use tracing::Instrument;
    use tracing::info_span;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    use std::time::Instant;

    use opentelemetry::KeyValue;
    use opentelemetry::global;
    use opentelemetry_semantic_conventions::resource::URL_PATH;
    use tonic::async_trait;
    use tonic::body::BoxBody;
    use tonic::codegen::http::Request as HttpRequest; // Use this instead of tonic::Request in Middleware!
    use tonic::codegen::http::Response as HttpResponse;
    use tonic_middleware::Middleware;
    use tonic_middleware::ServiceBound;

    use crate::tracing::propagation::HttpMetadataMap;

    pub const EXPORT_ORIGIN_HEADER: &str = "x-origin";

    /// MetricsMiddleware is a middleware that measures the latency of GRPC requests.
    #[derive(Clone, PartialEq)]
    pub struct MetricsMiddleware {
        skip_metrics_with_origin: Option<String>,
    }

    impl MetricsMiddleware {
        pub fn new() -> Self {
            MetricsMiddleware { skip_metrics_with_origin: None }
        }

        pub fn new_with_options(skip_metrics_with_origin: String) -> Self {
            MetricsMiddleware { skip_metrics_with_origin: Some(skip_metrics_with_origin) }
        }
    }

    #[async_trait]
    impl<S> Middleware<S> for MetricsMiddleware
    where
        S: ServiceBound,
        S::Future: Send,
    {
        async fn call(
            &self, req: HttpRequest<BoxBody>, mut service: S,
        ) -> Result<HttpResponse<BoxBody>, S::Error> {
            // If the options are not specified, we don't want to track it.
            let skip_metrics_with_origin = match self.skip_metrics_with_origin {
                Some(ref origin) => origin,
                None => return service.call(req).await,
            };
            // If the request originated from this service itself, we don't want to track it.
            let req_headers = req.headers();
            let should_skip_metrics = match req_headers.get(EXPORT_ORIGIN_HEADER) {
                Some(origin) => match origin.to_str() {
                    Ok(origin) => origin != skip_metrics_with_origin,
                    Err(e) => {
                        eprintln!("Failed to parse {} header: {}", EXPORT_ORIGIN_HEADER, e);
                        true
                    }
                },
                None => true,
            };
            if should_skip_metrics {
                return service.call(req).await;
            }

            // We should measure the duration of this request.
            let req_path = req.uri().path().to_owned();
            let start_time = Instant::now();
            let result = service.call(req).await;
            let elapsed_time = start_time.elapsed();

            // Send metrics events
            let meter = global::meter(format!("grpc"));
            meter
                .u64_counter("request.latency")
                .with_description("Latency of a GRPC request")
                .with_unit("microseconds")
                .init()
                .add(elapsed_time.as_micros() as u64, &[KeyValue::new(URL_PATH, req_path)]);

            result
        }
    }

    /// TracingMiddleware is a middleware that handles tracing context that is propagated across process boundaries.
    #[derive(Clone)]
    pub struct TracingMiddleware;

    #[async_trait]
    impl<S> Middleware<S> for TracingMiddleware
    where
        S: ServiceBound,
        S::Future: Send,
    {
        async fn call(
            &self, mut req: HttpRequest<BoxBody>, mut service: S,
        ) -> Result<HttpResponse<BoxBody>, S::Error> {
            // Extract the propagated tracing context from the incoming request headers.
            let parent_cx = global::get_text_map_propagator(|propagator| {
                propagator.extract(&HttpMetadataMap(req.headers_mut()))
            });

            // Extract correlation ID header (matches lit-api-core's extract_correlation_id implementation).
            // Priority: x-correlation-id > x-request-id
            let correlation_id = req
                .headers()
                .get("x-correlation-id")
                .or_else(|| req.headers().get("x-request-id"))
                .and_then(|h| h.to_str().ok())
                .filter(|s| !s.is_empty());

            // Initialize a new span with the propagated context as the parent.
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

            info_span.set_parent(parent_cx);

            service.call(req).instrument(info_span).await
        }
    }
}
