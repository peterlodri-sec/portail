use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::OnceLock;

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub fn global_metrics() -> &'static PrometheusHandle {
    METRICS_HANDLE.get_or_init(|| {
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .install_recorder()
            .expect("install metrics recorder")
    })
}
