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

pub mod a2a;
pub mod a2c;
pub mod auth;
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
pub mod fuzz_route;
pub mod gateway;
pub mod godfather;
pub mod graphql;
pub mod hooks;
pub mod mcp;
pub mod nats_bridge;
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
pub mod telemetry;
pub mod test_utils;
pub mod types;

pub use config::Config;

use std::sync::Arc;
use std::sync::RwLock;

pub struct AppState {
    pub config: RwLock<Config>,
    pub config_watcher: Arc<config_watcher::ConfigWatcher>,
    pub event_log: Arc<events::EventLog>,
    pub cdn_cache: Option<Arc<cdn::CacheManager>>,
    pub hooks: Arc<hooks::HookStore>,
    pub a2a_tasks: Arc<a2a::TaskStore>,
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
}
