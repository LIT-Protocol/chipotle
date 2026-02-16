use std::time::Duration;

use lit_core::error::Result;
use opentelemetry_otlp::TonicExporterBuilder;
use opentelemetry_sdk::{
    Resource,
    metrics::reader::{DefaultAggregationSelector, DefaultTemporalitySelector},
};

use crate::error::unexpected_err;

pub(crate) fn init_metrics_provider(
    tonic_exporter_builder: TonicExporterBuilder, resource: Resource,
) -> Result<opentelemetry_sdk::metrics::SdkMeterProvider> {
    opentelemetry_otlp::new_pipeline()
        .metrics(opentelemetry_sdk::runtime::Tokio)
        .with_exporter(tonic_exporter_builder)
        .with_period(Duration::from_secs(3))
        .with_timeout(Duration::from_secs(10))
        .with_resource(resource)
        .with_aggregation_selector(DefaultAggregationSelector::new())
        .with_temporality_selector(DefaultTemporalitySelector::new())
        .build()
        .map_err(|e| {
            unexpected_err(e.to_string(), Some("Could not build metrics pipeline".to_string()))
        })
}

pub trait LitMetric {
    fn get_meter(&self) -> &str;
    fn get_namespace(&self) -> &str;
    fn get_name(&self) -> &str;
    fn get_full_name(&self) -> String {
        format!("{}.{}", self.get_namespace(), self.get_name())
    }
    fn get_description(&self) -> &str;
    fn get_unit(&self) -> &str;
}

pub mod counter {
    use opentelemetry::{KeyValue, global};

    pub fn add_one(metric: impl super::LitMetric, attributes: &[KeyValue]) {
        add_value(metric, 1, attributes)
    }

    pub fn add_value(metric: impl super::LitMetric, value: u64, attributes: &[KeyValue]) {
        // shallow wrapper - this could probably all be cached?  What's the best practice?
        let meter = global::meter(metric.get_meter().to_string());
        let name = metric.get_full_name().to_string();
        let description = metric.get_description().to_string();
        let unit = metric.get_unit().to_owned();

        let mut counter = meter.u64_counter(name);

        if !description.is_empty() {
            counter = counter.with_description(description);
        }
        if !unit.is_empty() {
            counter = counter.with_unit(unit);
        }

        let counter = counter.init();
        counter.add(value, attributes);
    }
}
