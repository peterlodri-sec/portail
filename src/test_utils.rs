use std::sync::OnceLock;
use metrics_exporter_prometheus::PrometheusHandle;

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub fn global_metrics() -> &'static PrometheusHandle {
    METRICS_HANDLE.get_or_init(|| {
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .install_recorder()
            .expect("install metrics recorder")
    })
}
