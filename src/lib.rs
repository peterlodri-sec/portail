//! Portail - Unified Proxy/Gateway
//!
//! Architecture:
//!
//!   Client -> axum router -> middleware -> handler -> upstream
//!                   |
//!                   +-- /v1/chat/*  -> hooks.inject -> gateway.forward
//!                   +-- /mcp/*     -> mcp.proxy -> unix socket
//!                   +-- /cdn/*     -> cdn.lookup -> cache/origin
//!                   +-- /events/*  -> event_log -> SSE
//!                   +-- /hooks/*   -> hook_store CRUD
//!                   +-- /a2a/*     -> task lifecycle
//!                   +-- /a2c/*     -> chat API
//!                   +-- /dns/*     -> dns.resolve
//!                   +-- /tinyurl/* -> url shortening
//!                   +-- /traces/*  -> request tracing
//!                   +-- /cache/*   -> redis cache
//!
//! Rust 2024 Edition — native async fn in traits, resolver v3, unsafe_op_in_unsafe_fn.

#![deny(unsafe_op_in_unsafe_fn)]
#![forbid(unsafe_code)]
#![allow(clippy::too_many_arguments, clippy::large_enum_variant)]
#![allow(clippy::if_same_then_else, clippy::items_after_test_module)]
#![allow(clippy::new_without_default, clippy::needless_pass_by_value)]
#![allow(clippy::should_implement_trait, clippy::len_without_is_empty)]
#![allow(clippy::needless_borrow)]

pub mod a2a;
pub mod a2c;
pub mod auth;
pub mod base_hooks;
pub mod bow;
pub mod cdn;
pub mod ci;
pub mod cli;
pub mod config;
pub mod config_watcher;
pub mod discovery;
pub mod dns;
pub mod drift;
pub mod events;
pub mod file_cache;
pub mod gateway;
pub mod godfather;
pub mod graphql;
pub mod hooks;
pub mod local_inference;
pub mod mcp;
pub mod nats_bridge;
pub mod orchestrator;
pub mod plugin_hooks;
pub mod plugins;
pub mod proxy;
pub mod rate_limit;
pub mod release_audit;
pub mod sentinel;
pub mod sessions;
pub mod shutdown;
pub mod spec_verify;
pub mod store;
pub mod supervisor;
pub mod target_router;
pub mod telemetry;
pub mod test_utils;
pub mod types;

pub use config::Config;
use loop_state_manager::LoopStateManager;
use portail_vaked::PluginRegistry;

use std::sync::Arc;
use std::sync::RwLock;

pub struct AppState {
    pub config: RwLock<Config>,
    pub config_watcher: Arc<config_watcher::ConfigWatcher>,
    pub event_log: Arc<events::EventLog>,
    pub cdn_cache: Option<Arc<cdn::CacheManager>>,
    pub hooks: Arc<hooks::HookStore>,
    pub base_hooks: Arc<base_hooks::BaseHookRegistry>,
    pub a2a_tasks: Arc<a2a::TaskStore>,
    pub a2a_registry: Arc<a2a::registry::AgentRegistry>,
    pub dns_store: Arc<dns::DnsStore>,
    pub doh_client: Option<Arc<dns::DohClient>>,
    pub network_isolation: Arc<dns::NetworkIsolation>,
    pub tinyurl: Arc<plugins::TinyUrlStore>,
    pub trace_store: Arc<plugins::TraceStore>,
    pub redis_cache: Arc<plugins::RedisCache>,
    pub discovery: Arc<discovery::DiscoveryStore>,
    pub ci_status: Arc<ci::CiStatusStore>,
    pub metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
    pub rate_limiter: Option<rate_limit::RateLimiter>,
    pub auth_state: Option<auth::AuthState>,
    pub event_store: Option<store::EventStore>,
    pub session_store: sessions::SessionStore,
    pub file_cache: file_cache::FileCache,
    pub supervisor: Arc<supervisor::Supervisor>,
    pub plugin_registry: Arc<std::sync::Mutex<PluginRegistry>>,
    pub loop_manager: Arc<LoopStateManager>,
    pub loop_runner: loopeng::SharedLoopEngine,
    pub inference_engine: Option<Arc<local_inference::InferenceEngine>>,
    pub pkg_ctx_memory: tokio::sync::Mutex<pkg_ctx::memory::PkgCtxMemory>,
    pub tool_registry: Arc<std::sync::RwLock<portail_claude_plugins::bridge::ToolRegistry>>,
}

#[cfg(test)]
impl AppState {
    pub fn test_default() -> Self {
        let event_log = Arc::new(events::EventLog::new(1000));
        let supervisor = Arc::new(supervisor::Supervisor::new(event_log.clone()));

        Self {
            config: RwLock::new(Config::default()),
            config_watcher: config_watcher::ConfigWatcher::new("portail.toml".into()),
            event_log,
            cdn_cache: None,
            hooks: Arc::new(hooks::HookStore::new()),
            base_hooks: Arc::new(base_hooks::default_registry()),
            a2a_tasks: Arc::new(a2a::TaskStore::new()),
            a2a_registry: Arc::new(a2a::registry::AgentRegistry::new()),
            dns_store: Arc::new(dns::DnsStore::new()),
            doh_client: None,
            network_isolation: Arc::new(dns::NetworkIsolation::default()),
            tinyurl: Arc::new(plugins::TinyUrlStore::new(plugins::TinyUrlConfig::default())),
            trace_store: Arc::new(plugins::TraceStore::new(10000)),
            redis_cache: Arc::new(plugins::RedisCache::new(
                plugins::RedisCacheConfig::default(),
            )),
            discovery: Arc::new(discovery::DiscoveryStore::new(
                discovery::DiscoveryConfig::default(),
            )),
            ci_status: Arc::new(ci::CiStatusStore::new(100, None)),
            metrics_handle: crate::test_utils::global_metrics().clone(),
            rate_limiter: None,
            auth_state: None,
            event_store: None,
            session_store: sessions::SessionStore::new(100),
            file_cache: file_cache::FileCache::new(&file_cache::FileCacheConfig::default()),
            supervisor,
            plugin_registry: Arc::new(std::sync::Mutex::new(portail_vaked::PluginRegistry::new(
                "/tmp/portail-plugins".into(),
            ))),
            loop_manager: Arc::new(loop_state_manager::LoopStateManager::new("0.1.0")),
            loop_runner: loopeng::SharedLoopEngine::new(loopeng::LoopEngineConfig::default()),
            inference_engine: None,
            pkg_ctx_memory: tokio::sync::Mutex::new(pkg_ctx::memory::PkgCtxMemory::new().unwrap()),
            tool_registry: Arc::new(std::sync::RwLock::new(
                portail_claude_plugins::bridge::ToolRegistry::new(),
            )),
        }
    }
}
