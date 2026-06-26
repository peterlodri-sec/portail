pub mod cdn;
pub mod config;
pub mod gateway;
pub mod mcp;
pub mod proxy;

use config::Config;
use std::sync::{Arc, RwLock};

pub struct AppState {
    pub config: RwLock<Config>,
    pub cdn_cache: Option<Arc<cdn::CacheManager>>,
    pub metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
}
