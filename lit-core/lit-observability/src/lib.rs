use std::str::FromStr;

pub use config::LitObservabilityConfig;
use error::unexpected_err;
use lit_core::config::LitConfig;
use logging::{ContextAwareOtelLogLayer, CustomEventFormatter, init_logger_provider};
use metrics::init_metrics_provider;
use net::init_tonic_exporter_builder;
use opentelemetry::trace::TracerProvider;

use opentelemetry_sdk::logs::LoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::{Resource, trace as sdktrace};

use ::tracing::Subscriber;
use tracing::init_tracing_provider;
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
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

// Re-exports
pub use opentelemetry;
pub use opentelemetry_sdk;
pub use opentelemetry_semantic_conventions;
pub use tonic_middleware;

pub async fn create_providers(
    cfg: &LitConfig, resource: Resource, trace_config: sdktrace::Config,
    #[cfg(feature = "proxy-collector")] proxy_collector_name: &'static str,
) -> Result<(sdktrace::TracerProvider, SdkMeterProvider, impl Subscriber, LoggerProvider)> {
    // Initialize the tracing pipeline
    let tonic_exporter_builder = {
        #[cfg(feature = "proxy-collector")]
        {
            init_tonic_exporter_builder(cfg, proxy_collector_name).await?
        }
        #[cfg(not(feature = "proxy-collector"))]
        {
            init_tonic_exporter_builder(cfg).await?
        }
    };
    let tracing_provider = init_tracing_provider(tonic_exporter_builder, trace_config)?;
    let tracer = tracing_provider.tracer("lit-tracer");

    // Initialize the metrics pipeline
    let tonic_exporter_builder = {
        #[cfg(feature = "proxy-collector")]
        {
            init_tonic_exporter_builder(cfg, proxy_collector_name).await?
        }
        #[cfg(not(feature = "proxy-collector"))]
        {
            init_tonic_exporter_builder(cfg).await?
        }
    };
    let meter_provider = init_metrics_provider(tonic_exporter_builder, resource.clone())?;

    // Initialize the logs pipeline
    let tonic_exporter_builder = {
        #[cfg(feature = "proxy-collector")]
        {
            init_tonic_exporter_builder(cfg, proxy_collector_name).await?
        }
        #[cfg(not(feature = "proxy-collector"))]
        {
            init_tonic_exporter_builder(cfg).await?
        }
    };
    let logger_provider = init_logger_provider(tonic_exporter_builder, resource.clone())?;

    let context_aware_log_layer = ContextAwareOtelLogLayer::new(&logger_provider);

    // Add a tracing filter to filter events from crates used by opentelemetry-otlp.
    // The filter levels are set as follows:
    // - Allow the configured level and above by default.
    // - Restrict `hyper`, `tonic`, and `reqwest` to `error` level logs only.
    // This ensures events generated from these crates within the OTLP Exporter are not looped back,
    // thus preventing infinite event generation.
    // Note: This will also drop events from these crates used outside the OTLP Exporter.
    // For more details, see: https://github.com/open-telemetry/opentelemetry-rust/issues/761
    let cfg_log_level = cfg.logging_level()?;
    let level_filter = EnvFilter::try_from_default_env()
        .or_else(|_e| EnvFilter::from_str(cfg_log_level.as_str()))
        .map_err(|e| unexpected_err(e.to_string(), Some("Could not create filter".to_string())))?
        .add_directive("hyper=error".parse().unwrap())
        .add_directive("tonic=error".parse().unwrap())
        .add_directive("tower=error".parse().unwrap())
        .add_directive("h2=error".parse().unwrap())
        .add_directive("reqwest=error".parse().unwrap());

    let custom_formatter = CustomEventFormatter::default();

    let sub = tracing_subscriber::registry()
        .with(level_filter)
        .with(fmt::layer().event_format(custom_formatter))
        .with(context_aware_log_layer)
        .with(MetricsLayer::new(meter_provider.clone()))
        .with(OpenTelemetryLayer::new(tracer));

    let sub = sub.with(logging::privacy_filter::PrivacyModeLayer);

    Ok((tracing_provider, meter_provider, sub, logger_provider))
}
