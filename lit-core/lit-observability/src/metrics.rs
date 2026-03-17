#[cfg(feature = "otlp")]
use std::time::Duration;

#[cfg(feature = "otlp")]
use lit_core::error::Result;
#[cfg(feature = "otlp")]
use opentelemetry_otlp::TonicExporterBuilder;
#[cfg(feature = "otlp")]
use opentelemetry_sdk::{
    Resource,
    metrics::reader::{DefaultAggregationSelector, DefaultTemporalitySelector},
};

#[cfg(feature = "otlp")]
use crate::error::unexpected_err;

#[cfg(feature = "otlp")]
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
    use std::sync::OnceLock;

    use dashmap::DashMap;
    use opentelemetry::metrics::Counter;
    use opentelemetry::{KeyValue, global};

    static COUNTERS: OnceLock<DashMap<String, Counter<u64>>> = OnceLock::new();

    pub fn add_one(metric: impl super::LitMetric, attributes: &[KeyValue]) {
        add_value(metric, 1, attributes)
    }

    pub fn add_value(metric: impl super::LitMetric, value: u64, attributes: &[KeyValue]) {
        let counters = COUNTERS.get_or_init(DashMap::new);
        let name = metric.get_full_name();

        let counter = counters.entry(name.clone()).or_insert_with(|| {
            let meter = global::meter(metric.get_meter().to_string());
            let description = metric.get_description().to_string();
            let unit = metric.get_unit().to_owned();

            let mut builder = meter.u64_counter(name);

            if !description.is_empty() {
                builder = builder.with_description(description);
            }
            if !unit.is_empty() {
                builder = builder.with_unit(unit);
            }

            builder.init()
        });

        counter.add(value, attributes);
    }
}

pub mod gauge {
    use std::sync::OnceLock;

    use dashmap::DashMap;
    use opentelemetry::metrics::Gauge;
    use opentelemetry::{KeyValue, global};

    static GAUGES: OnceLock<DashMap<String, Gauge<u64>>> = OnceLock::new();

    pub fn record(metric: impl super::LitMetric, value: u64, attributes: &[KeyValue]) {
        let gauges = GAUGES.get_or_init(DashMap::new);
        let name = metric.get_full_name();

        let gauge = gauges.entry(name.clone()).or_insert_with(|| {
            let meter = global::meter(metric.get_meter().to_string());
            let description = metric.get_description().to_string();
            let unit = metric.get_unit().to_owned();

            let mut builder = meter.u64_gauge(name);

            if !description.is_empty() {
                builder = builder.with_description(description);
            }
            if !unit.is_empty() {
                builder = builder.with_unit(unit);
            }

            builder.init()
        });

        gauge.record(value, attributes);
    }
}
