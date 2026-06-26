//! OpenTelemetry OTLP export — traces to Jaeger/Tempo.
//!
//! v0.2 — gRPC-based OTLP trace export.

use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default = "default_sampling")]
    pub sampling_ratio: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_endpoint(),
            service_name: default_service_name(),
            sampling_ratio: default_sampling(),
        }
    }
}

fn default_endpoint() -> String { "http://localhost:4317".into() }
fn default_service_name() -> String { "portail".into() }
fn default_sampling() -> f64 { 0.1 }

pub struct OtelGuard {
    _provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

impl OtelGuard {
    pub fn shutdown(self) {
        if let Some(p) = self._provider { let _ = p.shutdown(); }
    }
}

pub fn init(config: &TelemetryConfig) -> Option<OtelGuard> {
    if !config.enabled { return None; }

    // OTLP endpoint is configured via the exporter builder, not env vars.
    // The env var approach is preferred by opentelemetry but set_var is
    // unsafe in multi-threaded contexts. We pass the endpoint directly.

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .with_timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let sampler = if config.sampling_ratio >= 1.0 {
        opentelemetry_sdk::trace::Sampler::AlwaysOn
    } else {
        opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(config.sampling_ratio)
    };

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name", config.service_name.clone(),
        )]))
        .with_sampler(sampler)
        .build();

    let _ = opentelemetry::global::set_tracer_provider(provider.clone());

    tracing::info!(
        endpoint = %config.endpoint, service = %config.service_name,
        sampling = config.sampling_ratio,
        "OpenTelemetry OTLP export enabled"
    );

    Some(OtelGuard { _provider: Some(provider) })
}
