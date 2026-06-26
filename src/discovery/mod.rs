/*
 * Network Discovery — Self-Service Discovery
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                Network Discovery Flow                       │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Agent Joins Network                                       │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  Register  │────▶│  Announce  │────▶│  Store     │     │
 *   │   │  (POST)    │     │  (mDNS)    │     │  (memory)  │     │
 *   │   └────────────┘     └────────────┘     └────────────┘     │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  Heartbeat │────▶│  Update    │────▶│  Expire    │     │
 *   │   │  (periodic)│     │  timestamp │     │  old nodes │     │
 *   │   └────────────┘     └────────────┘     └────────────┘     │
 *   │                                                             │
 *   │   Discovery Methods:                                        │
 *   │   1. HTTP API (POST /discovery/register)                    │
 *   │   2. mDNS/Bonjour (multicast)                              │
 *   │   3. DNS-SD (service discovery)                             │
 *   │   4. Static configuration                                   │
 *   │                                                             │
 *   │   Node Types:                                               │
 *   │   - agent     (AI agent)                                    │
 *   │   - service   (backend service)                             │
 *   │   - gateway   (portail instance)                            │
 *   │   - database  (data store)                                  │
 *   │   - cache     (redis/memcached)                             │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNode {
    pub id: String,
    pub name: String,
    pub node_type: NodeType,
    pub address: String,
    pub port: u16,
    pub protocol: Protocol,
    pub metadata: FxHashMap<String, String>,
    pub registered_at: u64,
    pub last_heartbeat: u64,
    pub status: NodeStatus,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Agent,
    Service,
    Gateway,
    Database,
    Cache,
    Monitor,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Http,
    Https,
    Tcp,
    Udp,
    Unix,
    Grpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Online,
    Degraded,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    pub node_expiry_secs: u64,
    pub mdns_enabled: bool,
    pub mdns_domain: String,
    pub dns_sd_enabled: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval_secs: 30,
            node_expiry_secs: 300, // 5 minutes
            mdns_enabled: true,
            mdns_domain: "_portail._tcp.local".into(),
            dns_sd_enabled: true,
        }
    }
}

// ── Discovery Store ──────────────────────────────────────────────

pub struct DiscoveryStore {
    nodes: std::sync::RwLock<FxHashMap<String, NetworkNode>>,
    config: DiscoveryConfig,
}

impl DiscoveryStore {
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            nodes: std::sync::RwLock::new(FxHashMap::default()),
            config,
        }
    }

    pub fn register(&self, node: NetworkNode) -> NetworkNode {
        let mut nodes = self.nodes.write().unwrap();
        let mut node = node;
        node.registered_at = now_millis();
        node.last_heartbeat = now_millis();
        node.status = NodeStatus::Online;
        nodes.insert(node.id.clone(), node.clone());
        node
    }

    pub fn heartbeat(&self, id: &str) -> Option<NetworkNode> {
        let mut nodes = self.nodes.write().unwrap();
        if let Some(node) = nodes.get_mut(id) {
            node.last_heartbeat = now_millis();
            node.status = NodeStatus::Online;
            Some(node.clone())
        } else {
            None
        }
    }

    pub fn deregister(&self, id: &str) -> bool {
        let mut nodes = self.nodes.write().unwrap();
        nodes.remove(id).is_some()
    }

    pub fn get(&self, id: &str) -> Option<NetworkNode> {
        let nodes = self.nodes.read().unwrap();
        nodes.get(id).cloned()
    }

    pub fn list(&self, node_type: Option<NodeType>) -> Vec<NetworkNode> {
        let nodes = self.nodes.read().unwrap();
        match node_type {
            Some(t) => nodes.values().filter(|n| n.node_type == t).cloned().collect(),
            None => nodes.values().cloned().collect(),
        }
    }

    pub fn expire_old(&self) -> usize {
        let mut nodes = self.nodes.write().unwrap();
        let now = now_millis();
        let expiry_ms = self.config.node_expiry_secs * 1000;
        let before = nodes.len();
        nodes.retain(|_, n| now - n.last_heartbeat < expiry_ms);
        before - nodes.len()
    }

    pub fn stats(&self) -> DiscoveryStats {
        let nodes = self.nodes.read().unwrap();
        let now = now_millis();
        let online = nodes.values().filter(|n| now - n.last_heartbeat < self.config.node_expiry_secs * 1000).count();
        
        DiscoveryStats {
            total_nodes: nodes.len(),
            online_nodes: online,
            offline_nodes: nodes.len() - online,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryStats {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub offline_nodes: usize,
}

// ── mDNS/SD Helpers ──────────────────────────────────────────────

pub fn mdns_service_name(_config: &DiscoveryConfig) -> String {
    "_portail._tcp.local.".to_string()
}

pub fn mdns_txt_record(node: &NetworkNode) -> Vec<(String, String)> {
    vec![
        ("id".into(), node.id.clone()),
        ("name".into(), node.name.clone()),
        ("type".into(), format!("{:?}", node.node_type)),
        ("protocol".into(), format!("{:?}", node.protocol)),
        ("port".into(), node.port.to_string()),
    ]
}

// ── Background Discovery Loop ────────────────────────────────────

pub async fn run_discovery(
    config: DiscoveryConfig,
    store: Arc<DiscoveryStore>,
    event_log: Arc<crate::events::EventLog>,
) {
    let interval = std::time::Duration::from_secs(config.heartbeat_interval_secs);

    tracing::info!("Network discovery service started");

    event_log.publish(crate::events::AgentEvent {
        agent_id: "discovery".into(),
        event_type: "started".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: rustc_hash::FxHashMap::from_iter([
            ("mdns_enabled".into(), config.mdns_enabled.to_string()),
            ("dns_sd_enabled".into(), config.dns_sd_enabled.to_string()),
        ]),
    });

    loop {
        tokio::time::sleep(interval).await;

        // Expire old nodes
        let expired = store.expire_old();
        if expired > 0 {
            event_log.publish(crate::events::AgentEvent {
                agent_id: "discovery".into(),
                event_type: "nodes_expired".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: rustc_hash::FxHashMap::from_iter([
                    ("expired".into(), expired.to_string()),
                ]),
            });
        }

        // Publish stats
        let stats = store.stats();
        event_log.publish(crate::events::AgentEvent {
            agent_id: "discovery".into(),
            event_type: "heartbeat".into(),
            severity: "info".into(),
            timestamp: 0,
            metadata: rustc_hash::FxHashMap::from_iter([
                ("total_nodes".into(), stats.total_nodes.to_string()),
                ("online_nodes".into(), stats.online_nodes.to_string()),
                ("offline_nodes".into(), stats.offline_nodes.to_string()),
            ]),
        });
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_register(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(node): axum::Json<NetworkNode>,
) -> impl axum::response::IntoResponse {
    let registered = state.discovery.register(node);
    (axum::http::StatusCode::CREATED, axum::Json(registered))
}

pub async fn handle_heartbeat(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.discovery.heartbeat(&id) {
        Some(node) => (axum::http::StatusCode::OK, axum::Json(serde_json::to_value(node).unwrap())),
        None => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": "not found"}))),
    }
}

pub async fn handle_deregister(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    if state.discovery.deregister(&id) {
        (axum::http::StatusCode::OK, "deregistered")
    } else {
        (axum::http::StatusCode::NOT_FOUND, "not found")
    }
}

pub async fn handle_list(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Query(params): axum::extract::Query<FxHashMap<String, String>>,
) -> axum::Json<Vec<NetworkNode>> {
    let node_type = params.get("type").and_then(|t| match t.as_str() {
        "agent" => Some(NodeType::Agent),
        "service" => Some(NodeType::Service),
        "gateway" => Some(NodeType::Gateway),
        "database" => Some(NodeType::Database),
        "cache" => Some(NodeType::Cache),
        "monitor" => Some(NodeType::Monitor),
        _ => None,
    });
    axum::Json(state.discovery.list(node_type))
}

pub async fn handle_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<DiscoveryStats> {
    axum::Json(state.discovery.stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/discovery/register", axum::routing::post(handle_register))
        .route("/discovery/heartbeat/{id}", axum::routing::post(handle_heartbeat))
        .route("/discovery/deregister/{id}", axum::routing::post(handle_deregister))
        .route("/discovery/nodes", axum::routing::get(handle_list))
        .route("/discovery/stats", axum::routing::get(handle_stats))
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

    fn test_node(id: &str) -> NetworkNode {
        NetworkNode {
            id: id.into(),
            name: format!("test-{}", id),
            node_type: NodeType::Agent,
            address: "127.0.0.1".into(),
            port: 8787,
            protocol: Protocol::Http,
            metadata: FxHashMap::default(),
            registered_at: 0,
            last_heartbeat: 0,
            status: NodeStatus::Unknown,
            tags: vec![],
        }
    }

    #[test]
    fn register_and_list() {
        let store = DiscoveryStore::new(DiscoveryConfig::default());
        store.register(test_node("node-1"));
        store.register(test_node("node-2"));
        
        let nodes = store.list(None);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn heartbeat_updates() {
        let store = DiscoveryStore::new(DiscoveryConfig::default());
        store.register(test_node("node-1"));
        
        let node = store.heartbeat("node-1").unwrap();
        assert!(matches!(node.status, NodeStatus::Online));
    }

    #[test]
    fn deregister() {
        let store = DiscoveryStore::new(DiscoveryConfig::default());
        store.register(test_node("node-1"));
        
        assert!(store.deregister("node-1"));
        assert!(store.get("node-1").is_none());
    }

    #[test]
    fn stats() {
        let store = DiscoveryStore::new(DiscoveryConfig::default());
        store.register(test_node("node-1"));
        store.register(test_node("node-2"));
        
        let stats = store.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.online_nodes, 2);
    }
}
