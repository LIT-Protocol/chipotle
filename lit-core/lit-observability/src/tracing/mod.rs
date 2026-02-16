use lit_core::error::Result;
use opentelemetry_otlp::TonicExporterBuilder;
use opentelemetry_sdk::{runtime, trace};

pub mod propagation;

use crate::error::unexpected_err;

pub(crate) fn init_tracing_provider(
    tonic_exporter_builder: TonicExporterBuilder, trace_config: trace::Config,
) -> Result<trace::TracerProvider> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(tonic_exporter_builder)
        .with_trace_config(trace_config)
        .install_batch(runtime::Tokio)
        .map_err(|e| {
            unexpected_err(e.to_string(), Some("Could not build tracing pipeline".to_string()))
        })
}
