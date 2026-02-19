use lit_core::error::Result;
#[cfg(feature = "otlp")]
use opentelemetry_otlp::TonicExporterBuilder;
use opentelemetry_sdk::Resource;

use crate::error::unexpected_err;

#[cfg(feature = "otlp")]
pub(crate) fn init_logger_provider(
    tonic_exporter_builder: TonicExporterBuilder, resource: Resource,
) -> Result<opentelemetry_sdk::logs::LoggerProvider> {
    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_exporter(tonic_exporter_builder)
        .with_resource(resource)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .map_err(|e| {
            unexpected_err(e.to_string(), Some("Could not build logging pipeline".to_string()))
        })
}

pub mod context_layer;
pub mod event_format;
pub mod privacy_filter;

pub use event_format::CustomEventFormatter;
pub use context_layer::ContextAwareOtelLogLayer;
