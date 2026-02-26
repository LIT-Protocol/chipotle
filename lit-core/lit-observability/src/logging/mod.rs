use std::str::FromStr;

use crate::{config::LitObservabilityConfig, logging::privacy_filter::PrivacyModeLayer};
use lit_core::{config::LitConfig, error::Result};
use opentelemetry_otlp::TonicExporterBuilder;
use opentelemetry_sdk::{Resource, runtime};
use tracing::Subscriber;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

use crate::error::unexpected_err;

mod context_layer;
mod event_format;
pub mod privacy_filter;
// Re-export context layer components for use by lit-node
pub use context_layer::{
    ContextAwareOtelLogLayer, RequestContext, clear_task_request_context, get_request_context,
    set_request_context,
};
pub use event_format::CustomEventFormatter;

/// Initialize a simple `tracing` subscriber that logs to stdout.
pub fn simple_logging_subscriber(
    cfg: &LitConfig, prefix_string: Option<String>,
) -> Result<impl Subscriber> {
    let cfg_log_level = cfg.logging_level()?;
    let level_filter = EnvFilter::try_from_default_env()
        .or_else(|_e| EnvFilter::from_str(cfg_log_level.as_str()))
        .map_err(|e| unexpected_err(e.to_string(), Some("Could not create filter".to_string())))?;
    println!("Using level filter: {}", level_filter);

    let custom_formatter = event_format::CustomEventFormatter::default()
        .with_target(true)
        .with_source_location(true)
        .with_event_scope(false)
        .with_prefix_string(prefix_string);

    Ok(tracing_subscriber::registry()
        .with(level_filter)
        .with(PrivacyModeLayer)
        .with(fmt::layer().event_format(custom_formatter)))
}

/// Initialize a `tracing` subscriber that logs to a file.
/// NOTE: This should ONLY be used during testing currently.
#[cfg(feature = "testing")]
pub fn simple_file_logging_subscriber(
    cfg: &LitConfig, prefix_string: Option<String>,
) -> Result<impl Subscriber> {
    let cfg_log_level = cfg.logging_level()?;
    let level_filter = EnvFilter::try_from_default_env()
        .or_else(|_e| EnvFilter::from_str(cfg_log_level.as_str()))
        .map_err(|e| unexpected_err(e.to_string(), Some("Could not create filter".to_string())))?;
    println!("Using level filter: {}", level_filter);

    let file_appender = tracing_appender::rolling::never(
        "./tests/test_logs",
        cfg.get_string("node.staker_address")?.to_lowercase(),
    );

    let custom_formatter = event_format::CustomEventFormatter::default()
        .with_target(true)
        .with_source_location(true)
        .with_event_scope(false)
        .with_prefix_string(prefix_string);

    Ok(tracing_subscriber::registry()
        .with(level_filter)
        .with(PrivacyModeLayer)
        .with(fmt::layer().event_format(custom_formatter.clone()))
        .with(fmt::layer().event_format(custom_formatter).with_writer(file_appender)))
}

pub(crate) fn init_logger_provider(
    tonic_exporter_builder: TonicExporterBuilder, resource: Resource,
) -> Result<opentelemetry_sdk::logs::LoggerProvider> {
    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_exporter(tonic_exporter_builder)
        .with_resource(resource)
        .install_batch(runtime::Tokio)
        .map_err(|e| {
            unexpected_err(e.to_string(), Some("Could not build logs pipeline".to_string()))
        })
}
