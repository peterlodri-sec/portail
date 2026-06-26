/*
 * NullClaw — Network-Native Agent
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    NullClaw Agent                           │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Always-on, network-layer agent embedded in Portail.       │
 *   │   Monitors all agent activity, produces heartbeats,         │
 *   │   logs network topology changes, and provides               │
 *   │   observability for the entire agent mesh.                  │
 *   │                                                             │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │                  NullClaw Core                       │   │
 *   │   │                                                     │   │
 *   │   │   ┌───────────┐  ┌───────────┐  ┌───────────┐      │   │
 *   │   │   │ Heartbeat │  │ Topology  │  │ Activity  │      │   │
 *   │   │   │ Generator │  │ Mapper    │  │ Logger    │      │   │
 *   │   │   └───────────┘  └───────────┘  └───────────┘      │   │
 *   │   │         │              │              │              │   │
 *   │   │         └──────────────┼──────────────┘              │   │
 *   │   │                        │                             │   │
 *   │   │                        ▼                             │   │
 *   │   │              ┌───────────────────┐                   │   │
 *   │   │              │  Event Log        │                   │   │
 *   │   │              │  (portail events) │                   │   │
 *   │   │              └───────────────────┘                   │   │
 *   │   │                                                     │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                                                             │
 *   │   Heartbeat every 10s:                                      │
 *   │   {                                                          │
 *   │     "agent": "nullclaw",                                    │
 *   │     "type": "heartbeat",                                    │
 *   │     "uptime": 3600,                                         │
 *   │     "agents_seen": 5,                                       │
 *   │     "requests_processed": 1234,                             │
 *   │     "cache_hit_rate": 0.85,                                 │
 *   │     "active_traces": 12,                                    │
 *   │     "topology": {                                           │
 *   │       "nodes": ["agent-1", "agent-2"],                      │
 *   │       "edges": [["agent-1", "agent-2"]]                     │
 *   │     }                                                       │
 *   │   }                                                          │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NullClawConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    pub agent_id: String,
    pub log_topology: bool,
    pub log_activity: bool,
}

impl Default for NullClawConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval_secs: 10,
            agent_id: "nullclaw".into(),
            log_topology: true,
            log_activity: true,
        }
    }
}

// ── Heartbeat Data ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Heartbeat {
    pub agent: String,
    pub heartbeat_type: String,
    pub timestamp: u64,
    pub uptime_secs: u64,
    pub agents_seen: usize,
    pub requests_processed: u64,
    pub cache_hit_rate: f64,
    pub active_traces: usize,
    pub active_hooks: usize,
    pub active_tasks: usize,
    pub topology: Topology,
    pub memory: MemoryInfo,
    pub network: NetworkInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Topology {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryInfo {
    pub event_log_size: usize,
    pub trace_count: usize,
    pub tinyurl_entries: usize,
    pub cache_keys: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkInfo {
    pub dns_queries: u64,
    pub dns_cache_hits: u64,
    pub proxy_requests: u64,
    pub upstream_errors: u64,
}

// ── NullClaw Agent ───────────────────────────────────────────────

pub struct NullClaw {
    config: NullClawConfig,
    started_at: Instant,
    requests_processed: AtomicU64,
    upstream_errors: AtomicU64,
    agents_seen: std::sync::RwLock<Vec<String>>,
}

impl NullClaw {
    pub fn new(config: NullClawConfig) -> Self {
        Self {
            config,
            started_at: Instant::now(),
            requests_processed: AtomicU64::new(0),
            upstream_errors: AtomicU64::new(0),
            agents_seen: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn record_request(&self) {
        self.requests_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.upstream_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_agent(&self, agent_id: &str) {
        let mut agents = self.agents_seen.write().unwrap();
        if !agents.iter().any(|a| a == agent_id) {
            agents.push(agent_id.to_string());
        }
    }

    pub fn generate_heartbeat(&self, state: &crate::AppState) -> Heartbeat {
        let uptime = self.started_at.elapsed().as_secs();
        let requests = self.requests_processed.load(Ordering::Relaxed);
        let errors = self.upstream_errors.load(Ordering::Relaxed);
        let agents = self.agents_seen.read().unwrap().clone();

        // Build topology from A2A tasks and events
        let topology = self.build_topology(state);

        // Calculate cache hit rate
        let cache_hit_rate = if requests > 0 {
            (requests - errors) as f64 / requests as f64
        } else {
            1.0
        };

        Heartbeat {
            agent: self.config.agent_id.clone(),
            heartbeat_type: "heartbeat".into(),
            timestamp: now_millis(),
            uptime_secs: uptime,
            agents_seen: agents.len(),
            requests_processed: requests,
            cache_hit_rate,
            active_traces: state.trace_store.stats().total_traces,
            active_hooks: state.hooks.list().len(),
            active_tasks: state.a2a_tasks.get_all().len(),
            topology,
            memory: MemoryInfo {
                event_log_size: state.event_log.count(),
                trace_count: state.trace_store.stats().total_traces,
                tinyurl_entries: state.tinyurl.get_stats().total_entries,
                cache_keys: 0, // TODO: Redis key count
            },
            network: NetworkInfo {
                dns_queries: 0, // TODO: DNS query counter
                dns_cache_hits: 0,
                proxy_requests: requests,
                upstream_errors: errors,
            },
        }
    }

    fn build_topology(&self, state: &crate::AppState) -> Topology {
        let agents = self.agents_seen.read().unwrap();
        let tasks = state.a2a_tasks.get_all();

        let mut nodes = agents.clone();
        let mut edges = Vec::new();

        // Add nodes from A2A tasks
        for task in &tasks {
            for msg in &task.messages {
                if let Some(agent_id) = msg.metadata.get("agent_id") {
                    if !nodes.contains(agent_id) {
                        nodes.push(agent_id.clone());
                    }
                }
            }
        }

        // Build edges from task relationships
        for task in &tasks {
            let source = task.metadata.get("source").cloned().unwrap_or_default();
            let target = task.metadata.get("target").cloned().unwrap_or_default();
            if !source.is_empty() && !target.is_empty() {
                edges.push((source, target));
            }
        }

        Topology { nodes, edges }
    }
}

// ── Background Runner ────────────────────────────────────────────

pub async fn run_nullclaw(
    config: NullClawConfig,
    state: Arc<crate::AppState>,
) {
    let agent = NullClaw::new(config.clone());
    let interval = std::time::Duration::from_secs(config.heartbeat_interval_secs);

    tracing::info!(
        agent = %config.agent_id,
        interval = %config.heartbeat_interval_secs,
        "NullClaw agent started"
    );

    // Publish started event
    state.event_log.publish(crate::events::AgentEvent {
        agent_id: config.agent_id.clone(),
        event_type: "started".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: rustc_hash::FxHashMap::from_iter([
            ("version".into(), env!("CARGO_PKG_VERSION").into()),
            ("interval".into(), config.heartbeat_interval_secs.to_string()),
        ]),
    });

    loop {
        tokio::time::sleep(interval).await;

        // Record observed agents from events
        let recent_events = state.event_log.recent(100);
        for event in &recent_events {
            agent.record_agent(&event.agent_id);
        }

        // Generate and publish heartbeat
        let heartbeat = agent.generate_heartbeat(&state);

        if config.log_activity {
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: config.agent_id.clone(),
                event_type: "heartbeat".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: rustc_hash::FxHashMap::from_iter([
                    ("uptime".into(), heartbeat.uptime_secs.to_string()),
                    ("agents_seen".into(), heartbeat.agents_seen.to_string()),
                    ("requests".into(), heartbeat.requests_processed.to_string()),
                    ("cache_hit_rate".into(), format!("{:.2}", heartbeat.cache_hit_rate)),
                    ("active_traces".into(), heartbeat.active_traces.to_string()),
                ]),
            });
        }

        // Log topology if enabled
        if config.log_topology && !heartbeat.topology.nodes.is_empty() {
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: config.agent_id.clone(),
                event_type: "topology".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: rustc_hash::FxHashMap::from_iter([
                    ("nodes".into(), heartbeat.topology.nodes.len().to_string()),
                    ("edges".into(), heartbeat.topology.edges.len().to_string()),
                    ("node_list".into(), heartbeat.topology.nodes.join(",")),
                ]),
            });
        }

        tracing::debug!(
            agent = %config.agent_id,
            uptime = %heartbeat.uptime_secs,
            agents = %heartbeat.agents_seen,
            requests = %heartbeat.requests_processed,
            "heartbeat"
        );
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_heartbeat(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<Heartbeat> {
    let config = NullClawConfig::default();
    let agent = NullClaw::new(config);
    axum::Json(agent.generate_heartbeat(&state))
}

pub async fn handle_agents(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<Vec<String>> {
    let recent = state.event_log.recent(1000);
    let mut agents: Vec<String> = recent.iter()
        .map(|e| e.agent_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    agents.sort();
    axum::Json(agents)
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/nullclaw/heartbeat", axum::routing::get(handle_heartbeat))
        .route("/nullclaw/agents", axum::routing::get(handle_agents))
}

// ── Helpers ──────────────────────────────────────────────────────

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullclaw_config_default() {
        let config = NullClawConfig::default();
        assert!(config.enabled);
        assert_eq!(config.heartbeat_interval_secs, 10);
        assert_eq!(config.agent_id, "nullclaw");
    }

    #[test]
    fn nullclaw_record_request() {
        let agent = NullClaw::new(NullClawConfig::default());
        agent.record_request();
        agent.record_request();
        assert_eq!(agent.requests_processed.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn nullclaw_record_agent() {
        let agent = NullClaw::new(NullClawConfig::default());
        agent.record_agent("agent-1");
        agent.record_agent("agent-2");
        agent.record_agent("agent-1"); // duplicate
        
        let agents = agent.agents_seen.read().unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn topology_generation() {
        let agent = NullClaw::new(NullClawConfig::default());
        agent.record_agent("web-agent");
        agent.record_agent("code-agent");
        
        let state = create_test_state();
        let heartbeat = agent.generate_heartbeat(&state);
        
        assert_eq!(heartbeat.topology.nodes.len(), 2);
        assert!(heartbeat.topology.nodes.contains(&"web-agent".into()));
        assert!(heartbeat.topology.nodes.contains(&"code-agent".into()));
    }

    fn create_test_state() -> crate::AppState {
        crate::AppState {
            config: std::sync::RwLock::new(crate::config::Config::default()),
            event_log: Arc::new(crate::events::EventLog::new(100)),
            cdn_cache: None,
            hooks: Arc::new(crate::hooks::HookStore::new()),
            a2a_tasks: Arc::new(crate::a2a::TaskStore::new()),
            dns_store: Arc::new(crate::dns::DnsStore::new()),
            doh_client: None,
            network_isolation: Arc::new(crate::dns::NetworkIsolation::default()),
            tinyurl: Arc::new(crate::plugins::TinyUrlStore::new(crate::plugins::TinyUrlConfig::default())),
            trace_store: Arc::new(crate::plugins::TraceStore::new(100)),
            redis_cache: Arc::new(crate::plugins::RedisCache::new(crate::plugins::RedisCacheConfig::default())),
            discovery: Arc::new(crate::discovery::DiscoveryStore::new(crate::discovery::DiscoveryConfig::default())),
            ebpf: Arc::new(crate::ebpf::EbpfManager::new(crate::ebpf::EbpfConfig::default())),
            iouring: Arc::new(crate::iouring::IoUringManager::new(crate::iouring::IoUringConfig::default())),
            dpdk: Arc::new(crate::dpdk::DpdkManager::new(crate::dpdk::DpdkConfig::default())),
            hyper: Arc::new(crate::hyper_engine::HyperManager::new(crate::hyper_engine::HyperConfig::default())),
            ci_status: Arc::new(crate::ci::CiStatusStore::new(100, None)),
            metrics_handle: crate::test_utils::global_metrics().clone(),
            rate_limiter: None,
            auth_state: None,
            event_store: None,
            session_store: crate::sessions::SessionStore::new(20),
            file_cache: crate::file_cache::FileCache::new(&crate::file_cache::FileCacheConfig { path: "/tmp/portail-test-cache".into(), ..Default::default() }),
        }
    }
}
