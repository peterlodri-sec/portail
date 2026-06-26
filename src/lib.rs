pub mod cdn;
pub mod config;
pub mod events;
pub mod gateway;
pub mod hooks;
pub mod mcp;
pub mod proxy;
pub mod sentinel;

pub use config::Config;

use std::sync::Arc;
use std::sync::RwLock;

pub struct AppState {
    pub config: RwLock<Config>,
    pub event_log: Arc<events::EventLog>,
    pub cdn_cache: Option<Arc<cdn::CacheManager>>,
    pub hooks: Arc<hooks::HookStore>,
    pub metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
}
