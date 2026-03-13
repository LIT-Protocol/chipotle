use std::str::FromStr;

use ::tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{fmt, prelude::*};

use lit_core::error::Result;
pub const PRIVACY_MODE_TAG: &str = "lit_privacy_mode";

#[cfg(feature = "channels")]
pub mod channels;
mod error;
pub mod logging;
pub mod metrics;
pub mod net;
pub mod tracing;

// Feature-gated re-exports
#[cfg(feature = "otlp")]
pub use opentelemetry;
#[cfg(feature = "otlp")]
pub use opentelemetry_sdk;
#[cfg(feature = "otlp")]
pub use opentelemetry_semantic_conventions;
#[cfg(feature = "otlp")]
pub use tracing_opentelemetry;
pub use tonic_middleware;

/// Initializes the primary tracing subscriber with fmt (stdout) and privacy filtering.
/// `log_level` is the minimum log level (e.g. "info", "debug"). Overridden by `RUST_LOG`.
pub fn init_subscriber(
    log_level: &str,
) -> Result<
    impl Subscriber + Send + Sync + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
> {
    let level_filter =
        EnvFilter::try_from_default_env().or_else(|_e| EnvFilter::from_str(log_level)).map_err(
            |e| error::unexpected_err(e.to_string(), Some("Could not create filter".to_string())),
        )?;

    let custom_formatter = logging::CustomEventFormatter::default();

    Ok(tracing_subscriber::registry()
        .with(level_filter)
        .with(fmt::layer().event_format(custom_formatter))
        .with(logging::privacy_filter::PrivacyModeLayer))
}

/// Feature-gated OTLP provider initialization.
/// `endpoint` is the OTLP/gRPC collector endpoint (e.g. "http://otel-collector:4317").
#[cfg(feature = "otlp")]
pub async fn create_providers(
    endpoint: &str, resource: opentelemetry_sdk::Resource,
    trace_config: opentelemetry_sdk::trace::Config,
) -> Result<(
    opentelemetry_sdk::trace::TracerProvider,
    opentelemetry_sdk::metrics::SdkMeterProvider,
    opentelemetry_sdk::logs::LoggerProvider,
)> {
    let tracing_provider =
        tracing::init_tracing_provider(net::init_tonic_exporter_builder(endpoint)?, trace_config)?;

    let meter_provider = metrics::init_metrics_provider(
        net::init_tonic_exporter_builder(endpoint)?,
        resource.clone(),
    )?;

    let logger_provider = logging::init_logger_provider(
        net::init_tonic_exporter_builder(endpoint)?,
        resource.clone(),
    )?;

    Ok((tracing_provider, meter_provider, logger_provider))
}
