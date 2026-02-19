use std::str::FromStr;

pub use config::LitObservabilityConfig;
use lit_core::config::LitConfig;

use ::tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{fmt, prelude::*};

use lit_core::error::Result;
pub const PRIVACY_MODE_TAG: &str = "lit_privacy_mode";

#[cfg(feature = "channels")]
pub mod channels;
mod config;
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
pub use tonic_middleware;

/// Initializes the primary tracing subscriber with fmt (stdout) and privacy filtering.
pub fn init_subscriber(cfg: &LitConfig) -> Result<impl Subscriber> {
    let cfg_log_level = cfg.get_string("logging.level").unwrap_or_else(|_| "info".to_string());
    let level_filter = EnvFilter::try_from_default_env()
        .or_else(|_e| EnvFilter::from_str(cfg_log_level.as_str()))
        .map_err(|e| error::unexpected_err(e.to_string(), Some("Could not create filter".to_string())))?
        .add_directive("hyper=error".parse().unwrap())
        .add_directive("tonic=error".parse().unwrap())
        .add_directive("tower=error".parse().unwrap())
        .add_directive("h2=error".parse().unwrap())
        .add_directive("reqwest=error".parse().unwrap());

    let custom_formatter = logging::CustomEventFormatter::default();

    Ok(tracing_subscriber::registry()
        .with(level_filter)
        .with(fmt::layer().event_format(custom_formatter))
        .with(logging::privacy_filter::PrivacyModeLayer))
}

/// Feature-gated OTLP provider initialization.
/// This provides the OTLP exporters that can be added as Layers to the tracing subscriber.
#[cfg(feature = "otlp")]
pub async fn create_providers(
    cfg: &LitConfig, 
    resource: opentelemetry_sdk::Resource, 
    trace_config: opentelemetry_sdk::trace::Config,
) -> Result<(opentelemetry_sdk::trace::TracerProvider, opentelemetry_sdk::metrics::SdkMeterProvider, opentelemetry_sdk::logs::LoggerProvider)> {
    // Initialize the tracing pipeline
    let tracing_provider = tracing::init_tracing_provider(net::init_tonic_exporter_builder(cfg)?, trace_config)?;

    // Initialize the metrics pipeline
    let meter_provider = metrics::init_metrics_provider(net::init_tonic_exporter_builder(cfg)?, resource.clone())?;

    // Initialize the logs pipeline
    let logger_provider = logging::init_logger_provider(net::init_tonic_exporter_builder(cfg)?, resource.clone())?;

    Ok((tracing_provider, meter_provider, logger_provider))
}
