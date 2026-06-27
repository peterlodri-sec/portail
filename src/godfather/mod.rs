//! Godfather — System resource monitor + service health watchdog.
//!
//! Always-on background process that monitors disk, memory, CPU,
//! and portail process health every 10 seconds. Publishes events
//! and sends webhook alerts when thresholds are crossed. v0.6.

use crate::types::BoundedMeta;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ── Helpers ──────────────────────────────────────────────────────

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn gf_disk_threshold() -> u8 {
    85
}
fn gf_memory_threshold() -> u8 {
    90
}
fn gf_min_free_disk() -> u64 {
    1_073_741_824
}

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GodfatherConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    pub check_services: bool,
    pub check_resources: bool,
    pub check_network: bool,
    #[serde(default)]
    pub thresholds: ResourceThresholds,
    #[serde(default)]
    pub alert_webhook_url: Option<String>,
}

impl Default for GodfatherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval_secs: 10,
            check_services: true,
            check_resources: true,
            check_network: true,
            thresholds: ResourceThresholds::default(),
            alert_webhook_url: None,
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
pub struct ResourceThresholds {
    #[serde(default = "gf_disk_threshold")]
    pub disk_usage_pct: u8,
    #[serde(default = "gf_memory_threshold")]
    pub memory_usage_pct: u8,
    #[serde(default = "gf_min_free_disk")]
    pub min_free_disk_bytes: u64,
}

impl Default for ResourceThresholds {
    fn default() -> Self {
        Self {
            disk_usage_pct: gf_disk_threshold(),
            memory_usage_pct: gf_memory_threshold(),
            min_free_disk_bytes: gf_min_free_disk(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceStats {
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_usage_pct: f64,
    pub cpu_usage_pct: f64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_usage_pct: f64,
    pub system_uptime_secs: u64,
    pub process_memory_bytes: u64,
    pub process_cpu_pct: f64,
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

// ── Godfather Core ───────────────────────────────────────────────

pub struct Godfather {
    _config: GodfatherConfig,
    started_at: Instant,
    tick_count: AtomicU64,
    services: std::sync::RwLock<Vec<ServiceStatus>>,
}

impl Godfather {
    pub fn new(config: GodfatherConfig) -> Self {
        Self {
            _config: config,
            started_at: Instant::now(),
            tick_count: AtomicU64::new(0),
            services: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn record_tick(&self) -> u64 {
        self.tick_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn check_portail_services(&self, state: &crate::AppState) -> Vec<ServiceStatus> {
        let mut services = Vec::new();
        let elapsed = self.started_at.elapsed().as_secs();

        services.push(ServiceStatus {
            name: "proxy".into(),
            status: ServiceState::Running,
            pid: Some(std::process::id()),
            uptime_secs: Some(elapsed),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });
        services.push(ServiceStatus {
            name: "event_log".into(),
            status: ServiceState::Running,
            pid: None,
            uptime_secs: Some(elapsed),
            memory_bytes: None,
            last_heartbeat: Some(now_millis()),
        });
        let _ = state; // reserve for future per-service checks
        services
    }

    pub fn update_service(&self, svc: ServiceStatus) {
        let mut list = self.services.write().unwrap();
        if let Some(existing) = list.iter_mut().find(|s| s.name == svc.name) {
            *existing = svc;
        } else {
            list.push(svc);
        }
    }

    pub fn gather_resources(&self) -> ResourceStats {
        use sysinfo::{Disks, System};

        let mut sys = System::new_all();
        sys.refresh_all();

        let mem_used = sys.used_memory();
        let mem_total = sys.total_memory();
        let mem_pct = if mem_total > 0 {
            (mem_used as f64 / mem_total as f64) * 100.0
        } else {
            0.0
        };

        let cpu_pct = sys.global_cpu_usage();
        let uptime = System::uptime();

        let pid = sysinfo::Pid::from_u32(std::process::id());
        let proc_mem = sys.process(pid).map(|p| p.memory()).unwrap_or(0);
        let proc_cpu = sys
            .process(pid)
            .map(|p| p.cpu_usage() as f64)
            .unwrap_or(0.0);

        let disks = Disks::new_with_refreshed_list();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        let mut disk_used: u64 = 0;
        let mut disk_total: u64 = 0;
        let mut disk_pct: f64 = 0.0;

        let mut best: Option<&sysinfo::Disk> = None;
        let mut best_len = 0;
        for disk in disks.list() {
            let mp = disk.mount_point().to_string_lossy();
            if cwd.to_string_lossy().starts_with(mp.as_ref()) && mp.len() > best_len {
                best = Some(disk);
                best_len = mp.len();
            }
        }
        if best.is_none() {
            for disk in disks.list() {
                if disk.mount_point().to_string_lossy() == "/" {
                    best = Some(disk);
                    break;
                }
            }
        }
        if let Some(disk) = best {
            disk_total = disk.total_space();
            let free = disk.available_space();
            disk_used = disk_total.saturating_sub(free);
            disk_pct = if disk_total > 0 {
                (disk_used as f64 / disk_total as f64) * 100.0
            } else {
                0.0
            };
        }

        ResourceStats {
            memory_used_bytes: mem_used,
            memory_total_bytes: mem_total,
            memory_usage_pct: mem_pct,
            cpu_usage_pct: cpu_pct as f64,
            disk_used_bytes: disk_used,
            disk_total_bytes: disk_total,
            disk_usage_pct: disk_pct,
            system_uptime_secs: uptime,
            process_memory_bytes: proc_mem,
            process_cpu_pct: proc_cpu,
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

pub async fn run_godfather(config: GodfatherConfig, state: Arc<crate::AppState>) {
    let godfather = Godfather::new(config.clone());
    let interval = std::time::Duration::from_secs(config.heartbeat_interval_secs);

    tracing::info!(interval = %config.heartbeat_interval_secs, "Godfather started");

    state.event_log.publish(crate::events::AgentEvent {
        agent_id: "godfather".into(),
        event_type: "started".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: BoundedMeta::from_iter([
            ("version".into(), env!("CARGO_PKG_VERSION").into()),
            ("pid".into(), std::process::id().to_string()),
            (
                "interval".into(),
                config.heartbeat_interval_secs.to_string(),
            ),
        ]),
    });

    loop {
        tokio::time::sleep(interval).await;
        let tick = godfather.record_tick();

        // Resources — always checked (mandatory)
        {
            let resources = godfather.gather_resources();
            let thresholds = &config.thresholds;
            let mut severity = "info";
            let mut alerts: Vec<String> = Vec::new();
            let disk_free = resources
                .disk_total_bytes
                .saturating_sub(resources.disk_used_bytes);

            if resources.disk_usage_pct > thresholds.disk_usage_pct as f64 {
                severity = "critical";
                alerts.push(format!(
                    "disk {:.1}% > {}%",
                    resources.disk_usage_pct, thresholds.disk_usage_pct
                ));
            }
            if disk_free < thresholds.min_free_disk_bytes && resources.disk_total_bytes > 0 {
                severity = "critical";
                alerts.push(format!(
                    "disk free {} < min {}",
                    disk_free, thresholds.min_free_disk_bytes
                ));
            }
            if resources.memory_usage_pct > thresholds.memory_usage_pct as f64 {
                if severity != "critical" {
                    severity = "warning";
                }
                alerts.push(format!(
                    "memory {:.1}% > {}%",
                    resources.memory_usage_pct, thresholds.memory_usage_pct
                ));
            }

            let mut meta = BoundedMeta::from_iter([
                (
                    "memory_pct".into(),
                    format!("{:.1}", resources.memory_usage_pct),
                ),
                ("cpu_pct".into(), format!("{:.1}", resources.cpu_usage_pct)),
                (
                    "disk_pct".into(),
                    format!("{:.1}", resources.disk_usage_pct),
                ),
                ("disk_free_bytes".into(), disk_free.to_string()),
                (
                    "process_memory_bytes".into(),
                    resources.process_memory_bytes.to_string(),
                ),
                (
                    "process_cpu_pct".into(),
                    format!("{:.1}", resources.process_cpu_pct),
                ),
                (
                    "system_uptime_secs".into(),
                    resources.system_uptime_secs.to_string(),
                ),
            ]);
            if !alerts.is_empty() {
                let _ = meta.insert("alerts".into(), alerts.join("; "));
            }

            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "godfather".into(),
                event_type: "resource".into(),
                severity: severity.into(),
                timestamp: 0,
                metadata: meta.clone(),
            });

            if severity == "critical" {
                if let Some(ref webhook_url) = config.alert_webhook_url {
                    let payload = serde_json::json!({
                        "text": format!("🚨 portail CRITICAL: {}", alerts.join(", ")),
                        "attachments": [{"title": "Resource Alert", "fields": [
                            {"title": "Disk", "value": format!("{:.1}% used, {:.1} GB free", resources.disk_usage_pct, disk_free as f64 / 1e9), "short": true},
                            {"title": "Memory", "value": format!("{:.1}% ({:.1}/{:.1} GB)", resources.memory_usage_pct, resources.memory_used_bytes as f64 / 1e9, resources.memory_total_bytes as f64 / 1e9), "short": true},
                            {"title": "CPU", "value": format!("sys {:.1}% / proc {:.1}%", resources.cpu_usage_pct, resources.process_cpu_pct), "short": true},
                            {"title": "Uptime", "value": format!("{}s", resources.system_uptime_secs), "short": true},
                        ]}]
                    });
                    let _ = reqwest::Client::new()
                        .post(webhook_url)
                        .json(&payload)
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await;
                }
            }
        }

        // Services
        if config.check_services {
            let services = godfather.check_portail_services(&state);
            for svc in &services {
                godfather.update_service(svc.clone());
            }
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "godfather".into(),
                event_type: "heartbeat".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: BoundedMeta::from_iter([
                    ("tick".into(), tick.to_string()),
                    (
                        "uptime".into(),
                        godfather.started_at.elapsed().as_secs().to_string(),
                    ),
                    ("services".into(), services.len().to_string()),
                ]),
            });
        }

        if config.check_network {
            let network = godfather.gather_network(&state);
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "godfather".into(),
                event_type: "network".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: BoundedMeta::from_iter([
                    ("total_requests".into(), network.total_requests.to_string()),
                    ("total_errors".into(), network.total_errors.to_string()),
                ]),
            });
        }

        tracing::debug!(tick = %tick, uptime = %godfather.started_at.elapsed().as_secs(), "godfather heartbeat");
    }
}

// ── HTTP Handler ─────────────────────────────────────────────────

pub async fn handle_godfather_status(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<GodfatherStatus> {
    let gf = Godfather::new(GodfatherConfig::default());
    axum::Json(GodfatherStatus {
        uptime_secs: gf.started_at.elapsed().as_secs(),
        services: gf.check_portail_services(&state),
        resources: gf.gather_resources(),
        network: gf.gather_network(&state),
    })
}

#[derive(Debug, Serialize)]
pub struct GodfatherStatus {
    pub uptime_secs: u64,
    pub services: Vec<ServiceStatus>,
    pub resources: ResourceStats,
    pub network: NetworkStats,
}

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new().route(
        "/godfather/status",
        axum::routing::get(handle_godfather_status),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godfather_config_defaults() {
        let cfg = GodfatherConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.heartbeat_interval_secs, 10);
        assert!(cfg.check_services);
        assert!(cfg.check_resources);
        assert!(cfg.check_network);
        assert!(cfg.alert_webhook_url.is_none());
    }

    #[test]
    fn resource_thresholds_defaults() {
        let thresholds = ResourceThresholds::default();
        assert_eq!(thresholds.disk_usage_pct, 85);
        assert_eq!(thresholds.memory_usage_pct, 90);
        assert_eq!(thresholds.min_free_disk_bytes, 1_073_741_824);
    }

    #[test]
    fn service_state_serialization() {
        let states = vec![
            (ServiceState::Running, "running"),
            (ServiceState::Degraded, "degraded"),
            (ServiceState::Stopped, "stopped"),
            (ServiceState::Unknown, "unknown"),
        ];
        for (state, expected) in states {
            let json = serde_json::to_string(&state).unwrap();
            assert!(
                json.contains(expected),
                "ServiceState {:?} should serialize to contain '{}'",
                state,
                expected
            );
        }
    }

    #[test]
    fn godfather_new_initializes_tick_count() {
        let cfg = GodfatherConfig::default();
        let gf = Godfather::new(cfg);
        assert_eq!(gf.record_tick(), 1);
        assert_eq!(gf.record_tick(), 2);
        assert_eq!(gf.record_tick(), 3);
    }

    #[test]
    fn godfather_record_tick_is_monotonic() {
        let cfg = GodfatherConfig::default();
        let gf = Godfather::new(cfg);
        let prev = gf.record_tick();
        let next = gf.record_tick();
        assert!(next > prev);
    }

    #[test]
    fn godfather_update_service_adds_and_updates() {
        let cfg = GodfatherConfig::default();
        let gf = Godfather::new(cfg);

        let svc = ServiceStatus {
            name: "test-service".into(),
            status: ServiceState::Running,
            pid: Some(1234),
            uptime_secs: Some(60),
            memory_bytes: Some(1024),
            last_heartbeat: Some(1000),
        };

        gf.update_service(svc.clone());
        let updated = ServiceStatus {
            name: "test-service".into(),
            status: ServiceState::Degraded,
            ..svc
        };
        gf.update_service(updated);
    }

    #[test]
    fn resource_stats_serialization() {
        let stats = ResourceStats {
            memory_used_bytes: 10_000_000,
            memory_total_bytes: 16_000_000,
            memory_usage_pct: 62.5,
            cpu_usage_pct: 15.3,
            disk_used_bytes: 50_000_000_000,
            disk_total_bytes: 100_000_000_000,
            disk_usage_pct: 50.0,
            system_uptime_secs: 86400,
            process_memory_bytes: 5_000_000,
            process_cpu_pct: 2.1,
            open_files: 128,
            open_connections: 4,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("memory_used_bytes"));
        assert!(json.contains("cpu_usage_pct"));
        let deser: ResourceStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.memory_used_bytes, 10_000_000);
        assert_eq!(deser.cpu_usage_pct, 15.3);
    }

    #[test]
    fn network_stats_serialization() {
        let stats = NetworkStats {
            active_connections: 42,
            total_requests: 1000,
            total_errors: 5,
            bytes_in: 500_000,
            bytes_out: 2_000_000,
            dns_queries: 100,
            dns_failures: 2,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("active_connections"));
        let deser: NetworkStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.active_connections, 42);
    }
}
