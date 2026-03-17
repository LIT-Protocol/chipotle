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

// ---------------------------------------------------------------------------
// OTel-backed `metrics` facade recorder
// ---------------------------------------------------------------------------

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use opentelemetry::{KeyValue, global};

const METER_NAME: &str = "lit";

fn key_to_attributes(key: &Key) -> Vec<KeyValue> {
    key.labels().map(|l| KeyValue::new(l.key().to_string(), l.value().to_string())).collect()
}

// --- Counter bridge ---

struct OtelCounter {
    inner: opentelemetry::metrics::Counter<u64>,
    attributes: Vec<KeyValue>,
}

impl CounterFn for OtelCounter {
    fn increment(&self, value: u64) {
        self.inner.add(value, &self.attributes);
    }

    fn absolute(&self, _value: u64) {
        // OTel counters are additive-only; absolute set is not supported.
    }
}

// --- Gauge bridge ---

struct OtelGauge {
    inner: opentelemetry::metrics::Gauge<f64>,
    attributes: Vec<KeyValue>,
    current: AtomicU64, // stores f64 bits
}

impl GaugeFn for OtelGauge {
    fn increment(&self, value: f64) {
        loop {
            let bits = self.current.load(Ordering::Relaxed);
            let new = f64::from_bits(bits) + value;
            if self
                .current
                .compare_exchange_weak(bits, new.to_bits(), Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                self.inner.record(new, &self.attributes);
                break;
            }
        }
    }

    fn decrement(&self, value: f64) {
        self.increment(-value);
    }

    fn set(&self, value: f64) {
        self.current.store(value.to_bits(), Ordering::Relaxed);
        self.inner.record(value, &self.attributes);
    }
}

// --- Histogram bridge ---

struct OtelHistogram {
    inner: opentelemetry::metrics::Histogram<f64>,
    attributes: Vec<KeyValue>,
}

impl HistogramFn for OtelHistogram {
    fn record(&self, value: f64) {
        self.inner.record(value, &self.attributes);
    }
}

// --- Recorder ---

struct OtelRecorder {
    otel_counters: DashMap<String, opentelemetry::metrics::Counter<u64>>,
    otel_gauges: DashMap<String, opentelemetry::metrics::Gauge<f64>>,
    otel_histograms: DashMap<String, opentelemetry::metrics::Histogram<f64>>,
    descriptions: DashMap<String, String>,
}

impl OtelRecorder {
    fn new() -> Self {
        Self {
            otel_counters: DashMap::new(),
            otel_gauges: DashMap::new(),
            otel_histograms: DashMap::new(),
            descriptions: DashMap::new(),
        }
    }

    fn get_description(&self, name: &str) -> Option<String> {
        self.descriptions.get(name).map(|d| d.value().clone())
    }
}

impl Recorder for OtelRecorder {
    fn describe_counter(&self, key: KeyName, _unit: Option<Unit>, description: SharedString) {
        self.descriptions.insert(key.as_str().to_string(), description.to_string());
    }

    fn describe_gauge(&self, key: KeyName, _unit: Option<Unit>, description: SharedString) {
        self.descriptions.insert(key.as_str().to_string(), description.to_string());
    }

    fn describe_histogram(&self, key: KeyName, _unit: Option<Unit>, description: SharedString) {
        self.descriptions.insert(key.as_str().to_string(), description.to_string());
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        let name = key.name().to_string();
        let attributes = key_to_attributes(key);

        let otel_counter = self
            .otel_counters
            .entry(name.clone())
            .or_insert_with(|| {
                let meter = global::meter(METER_NAME);
                let mut builder = meter.u64_counter(name.clone());
                if let Some(desc) = self.get_description(&name) {
                    builder = builder.with_description(desc);
                }
                builder.init()
            })
            .clone();

        Counter::from_arc(Arc::new(OtelCounter { inner: otel_counter, attributes }))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        let name = key.name().to_string();
        let attributes = key_to_attributes(key);

        let otel_gauge = self
            .otel_gauges
            .entry(name.clone())
            .or_insert_with(|| {
                let meter = global::meter(METER_NAME);
                let mut builder = meter.f64_gauge(name.clone());
                if let Some(desc) = self.get_description(&name) {
                    builder = builder.with_description(desc);
                }
                builder.init()
            })
            .clone();

        Gauge::from_arc(Arc::new(OtelGauge {
            inner: otel_gauge,
            attributes,
            current: AtomicU64::new(0f64.to_bits()),
        }))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        let name = key.name().to_string();
        let attributes = key_to_attributes(key);

        let otel_histogram = self
            .otel_histograms
            .entry(name.clone())
            .or_insert_with(|| {
                let meter = global::meter(METER_NAME);
                let mut builder = meter.f64_histogram(name.clone());
                if let Some(desc) = self.get_description(&name) {
                    builder = builder.with_description(desc);
                }
                builder.init()
            })
            .clone();

        Histogram::from_arc(Arc::new(OtelHistogram { inner: otel_histogram, attributes }))
    }
}

/// Install the OTel-backed metrics recorder as the global `metrics` facade
/// backend. Call this after the OTel `MeterProvider` has been set globally.
pub fn install_recorder() {
    if let Err(e) = metrics::set_global_recorder(OtelRecorder::new()) {
        tracing::warn!("metrics recorder already installed: {e}");
    }
}
