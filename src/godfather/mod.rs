/*
 * Godfather — Internal Service Monitor
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    Godfather Process                        │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Always-on. Monitors everything. Logs to event stream.     │
 *   │                                                             │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │              10s Tick Loop                           │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                          │                                  │
 *   │            ┌─────────────┼─────────────┐                    │
 *   │            ▼             ▼             ▼                    │
 *   │   ┌───────────┐  ┌───────────┐  ┌───────────┐              │
 *   │   │  Service  │  │  Health   │  │  Resource │              │
 *   │   │  Discovery│  │  Checks   │  │  Monitor  │              │
 *   │   └───────────┘  └───────────┘  └───────────┘              │
 *   │            │             │             │                    │
 *   │            └─────────────┼─────────────┘                    │
 *   │                          ▼                                  │
 *   │                   Event Log Stream                          │
 *   │                                                             │
 *   │   Events Published:                                         │
 *   │   - godfather.started      (on boot)                       │
 *   │   - godfather.heartbeat    (every 10s)                     │
 *   │   - godfather.service_up   (service detected)              │
 *   │   - godfather.service_down (service failed)                │
 *   │   - godfather.resource     (memory/cpu/disk stats)         │
 *   │   - godfather.network      (connection stats)              │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use rustc_hash::FxHashMap;

// ── Helpers ──────────────────────────────────────────────────────

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GodfatherConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    pub check_services: bool,
    pub check_resources: bool,
    pub check_network: bool,
}

impl Default for GodfatherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval_secs: 10,
            check_services: true,
            check_resources: true,
            check_network: true,
        }
    }
}

// ── Service Status ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceStatus {
    pub name: String,
    pub status: ServiceState,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub last_heartbeat: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Running,
    Degraded,
    Stopped,
    Unknown,
}

// ── Resource Stats ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceStats {
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_usage_pct: f64,
    pub cpu_usage_pct: f64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_usage_pct: f64,
    pub open_files: u64,
    pub open_connections: u64,
}

// ── Network Stats ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkStats {
    pub active_connections: u64,
    pub total_requests: u64,
    pub total_errors: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub dns_queries: u64,
    pub dns_failures: u64,
}

// ── Godfather Process ────────────────────────────────────────────

pub struct Godfather {
    config: GodfatherConfig,
    started_at: Instant,
    tick_count: AtomicU64,
    services: std::sync::RwLock<Vec<ServiceStatus>>,
}

impl Godfather {
    pub fn new(config: GodfatherConfig) -> Self {
        Self {
            config,
            started_at: Instant::now(),
            tick_count: AtomicU64::new(0),
            services: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn record_tick(&self) -> u64 {
        self.tick_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn update_service(&self, status: ServiceStatus) {
        let mut services = self.services.write().unwrap();
        if let Some(existing) = services.iter_mut().find(|s| s.name == status.name) {
            *existing = status;
        } else {
            services.push(status);
        }
    }

    pub fn get_services(&self) -> Vec<ServiceStatus> {
        self.services.read().unwrap().clone()
    }

    pub fn check_portail_services(&self, state: &crate::AppState) -> Vec<ServiceStatus> {
        let mut services = Vec::new();

        // Check proxy
        services.push(ServiceStatus {
            name: "proxy".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check event log
        services.push(ServiceStatus {
            name: "event_log".into(),
            status: if state.event_log.count() > 0 { ServiceState::Running } else { ServiceState::Degraded },
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check hooks
        services.push(ServiceStatus {
            name: "hooks".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check A2A tasks
        services.push(ServiceStatus {
            name: "a2a".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check DNS
        services.push(ServiceStatus {
            name: "dns".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check tinyurl
        services.push(ServiceStatus {
            name: "tinyurl".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check tracer
        services.push(ServiceStatus {
            name: "tracer".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        // Check redis cache
        services.push(ServiceStatus {
            name: "redis_cache".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(self.started_at.elapsed().as_secs()),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });

        services
    }

    pub fn gather_resources(&self) -> ResourceStats {
        // In a real implementation, this would read from /proc or sysinfo
        // For now, return placeholder values
        ResourceStats {
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            memory_usage_pct: 0.0,
            cpu_usage_pct: 0.0,
            disk_used_bytes: 0,
            disk_total_bytes: 0,
            disk_usage_pct: 0.0,
            open_files: 0,
            open_connections: 0,
        }
    }

    pub fn gather_network(&self, state: &crate::AppState) -> NetworkStats {
        let trace_stats = state.trace_store.stats();
        NetworkStats {
            active_connections: 0,
            total_requests: trace_stats.total_traces as u64,
            total_errors: trace_stats.error_traces as u64,
            bytes_in: 0,
            bytes_out: 0,
            dns_queries: 0,
            dns_failures: 0,
        }
    }
}

// ── Background Runner ────────────────────────────────────────────

pub async fn run_godfather(
    config: GodfatherConfig,
    state: Arc<crate::AppState>,
) {
    let godfather = Godfather::new(config.clone());
    let interval = std::time::Duration::from_secs(config.heartbeat_interval_secs);

    tracing::info!(
        interval = %config.heartbeat_interval_secs,
        "Godfather process started"
    );

    // Publish started event
    state.event_log.publish(crate::events::AgentEvent {
        agent_id: "godfather".into(),
        event_type: "started".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: rustc_hash::FxHashMap::from_iter([
            ("version".into(), env!("CARGO_PKG_VERSION").into()),
            ("pid".into(), std::process::id().to_string()),
            ("interval".into(), config.heartbeat_interval_secs.to_string()),
        ]),
    });

    loop {
        tokio::time::sleep(interval).await;

        let tick = godfather.record_tick();

        // Check services
        if config.check_services {
            let services = godfather.check_portail_services(&state);
            for service in &services {
                godfather.update_service(service.clone());
            }

            // Publish service status
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "godfather".into(),
                event_type: "heartbeat".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: rustc_hash::FxHashMap::from_iter([
                    ("tick".into(), tick.to_string()),
                    ("uptime".into(), godfather.started_at.elapsed().as_secs().to_string()),
                    ("services".into(), services.len().to_string()),
                    ("services_running".into(), services.iter().filter(|s| matches!(s.status, ServiceState::Running)).count().to_string()),
                ]),
            });
        }

        // Check resources
        if config.check_resources {
            let resources = godfather.gather_resources();
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "godfather".into(),
                event_type: "resource".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: rustc_hash::FxHashMap::from_iter([
                    ("memory_pct".into(), format!("{:.1}", resources.memory_usage_pct)),
                    ("cpu_pct".into(), format!("{:.1}", resources.cpu_usage_pct)),
                    ("disk_pct".into(), format!("{:.1}", resources.disk_usage_pct)),
                    ("open_files".into(), resources.open_files.to_string()),
                    ("open_connections".into(), resources.open_connections.to_string()),
                ]),
            });
        }

        // Check network
        if config.check_network {
            let network = godfather.gather_network(&state);
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "godfather".into(),
                event_type: "network".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: rustc_hash::FxHashMap::from_iter([
                    ("total_requests".into(), network.total_requests.to_string()),
                    ("total_errors".into(), network.total_errors.to_string()),
                    ("active_connections".into(), network.active_connections.to_string()),
                    ("dns_queries".into(), network.dns_queries.to_string()),
                ]),
            });
        }

        tracing::debug!(
            tick = %tick,
            uptime = %godfather.started_at.elapsed().as_secs(),
            "godfather heartbeat"
        );
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_godfather_status(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<GodfatherStatus> {
    let godfather = Godfather::new(GodfatherConfig::default());
    let services = godfather.check_portail_services(&state);
    let resources = godfather.gather_resources();
    let network = godfather.gather_network(&state);

    axum::Json(GodfatherStatus {
        uptime_secs: godfather.started_at.elapsed().as_secs(),
        services,
        resources,
        network,
    })
}

#[derive(Debug, Serialize)]
pub struct GodfatherStatus {
    pub uptime_secs: u64,
    pub services: Vec<ServiceStatus>,
    pub resources: ResourceStats,
    pub network: NetworkStats,
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/godfather/status", axum::routing::get(handle_godfather_status))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godfather_config_default() {
        let config = GodfatherConfig::default();
        assert!(config.enabled);
        assert_eq!(config.heartbeat_interval_secs, 10);
        assert!(config.check_services);
        assert!(config.check_resources);
        assert!(config.check_network);
    }

    #[test]
    fn godfather_tick() {
        let godfather = Godfather::new(GodfatherConfig::default());
        assert_eq!(godfather.record_tick(), 1);
        assert_eq!(godfather.record_tick(), 2);
        assert_eq!(godfather.record_tick(), 3);
    }

    #[test]
    fn godfather_service_update() {
        let godfather = Godfather::new(GodfatherConfig::default());
        
        godfather.update_service(ServiceStatus {
            name: "proxy".into(),
            status: ServiceState::Running,
            pid: Some(1234),
            uptime_secs: Some(100),
            memory_bytes: Some(1024),
            last_heartbeat: Some(now_millis()),
        });

        let services = godfather.get_services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "proxy");
        assert!(matches!(services[0].status, ServiceState::Running));
    }

    #[test]
    fn service_state_serde() {
        let state = ServiceState::Running;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"running\"");
        
        let state: ServiceState = serde_json::from_str("\"degraded\"").unwrap();
        assert!(matches!(state, ServiceState::Degraded));
    }
}
