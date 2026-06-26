//! Persistent event store — sqlx-backed with optional NATS replication.
//!
//! # v2.x
//!
//! Primary backend: sqlx (async, compile-time checked, migration system).
//! Optional NATS replication: multi-node eventual consistency.
//! Deprecated: rusqlite (kept for backwards compat, not recommended).
//!
//! # Configuration
//!
//! ```toml
//! [store]
//! enabled = true
//! provider = "sqlx"  # "sqlx" (default) | "nats"
//! db_path = "/var/lib/portail/events.db"
//! ```

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod queries;
pub mod schema;
pub mod rusqlite_backend;
pub mod sqlx_backend;
pub mod nats_backend;

pub use rusqlite_backend::RusqliteBackend;
pub use sqlx_backend::SqlxBackend;
#[cfg(feature = "store-nats")]
pub use nats_backend::NatsReplicatedBackend;

// ─── Config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoreConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_provider")]
    pub provider: String,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: default_db_path(),
            retention_days: default_retention(),
            provider: default_provider(),
        }
    }
}

fn default_db_path() -> String { "/var/lib/portail/events.db".into() }
fn default_retention() -> u32 { 30 }
fn default_provider() -> String { "sqlx".into() }

// ─── Event Model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredEvent {
    pub id: Option<i64>,
    pub agent_id: String,
    pub event_type: String,
    pub severity: String,
    pub timestamp: i64,
    pub metadata_json: String,
}

// ─── Backend Trait ────────────────────────────────────────────────

pub trait StoreBackend: Send + Sync + 'static {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String>;
    fn query(
        &self, agent_id: Option<&str>, event_type: Option<&str>,
        since: Option<i64>, limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String>;
    fn count(&self) -> Result<i64, String>;
    fn purge_expired(&self, retention_days: u32) -> Result<usize, String>;
    fn export_json(&self, since: Option<i64>) -> Result<String, String>;
}

// ─── Facade ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct EventStore {
    backend: Arc<dyn StoreBackend>,
    config: StoreConfig,
}

impl EventStore {
    pub async fn open(config: StoreConfig) -> Result<Self, String> {
        let backend: Arc<dyn StoreBackend> = match config.provider.as_str() {
            #[cfg(feature = "store-nats")]
            "nats" => {
                tracing::info!("opening NATS-replicated event store");
                Arc::new(NatsReplicatedBackend::open(&config).await?)
            }
            "sqlx" => {
                tracing::info!(path = %config.db_path, "opening sqlx async event store");
                Arc::new(SqlxBackend::open(&config).await?)
            }
            _ => {
                tracing::info!(path = %config.db_path, "opening rusqlite event store");
                Arc::new(RusqliteBackend::open(&config)?)
            }
        };
        let store = Self { backend, config: config.clone() };
        store.start_retention();
        Ok(store)
    }

    /// Create an EventStore from an already-opened backend (for tests).
    pub fn from_backend(backend: Arc<dyn StoreBackend>, config: StoreConfig) -> Self {
        let store = Self { backend, config };
        store.start_retention();
        store
    }

    fn start_retention(&self) {
        if self.config.retention_days > 0 {
            let s = self.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    if let Err(e) = s.backend.purge_expired(s.config.retention_days) {
                        tracing::warn!("event store retention purge failed: {}", e);
                    }
                }
            });
        }
    }

    pub fn insert(&self, event: &StoredEvent) -> Result<i64, String> { self.backend.insert(event) }
    pub fn query(
        &self, a: Option<&str>, e: Option<&str>, s: Option<i64>, l: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String> { self.backend.query(a, e, s, l) }
    pub fn count(&self) -> Result<i64, String> { self.backend.count() }
    pub fn export_json(&self, s: Option<i64>) -> Result<String, String> { self.backend.export_json(s) }
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.to_string_lossy().to_string() + &path[1..];
        }
    }
    path.to_string()
}

// ─── PersistedHistory (for CLI rollback) ─────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedHistory {
    pub versions: Vec<PersistedVersion>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedVersion {
    pub version: u64,
    pub loaded_at: String,
    pub config_json: String,
}

impl PersistedHistory {
    pub fn load(config_path: &std::path::Path) -> Option<Self> {
        let mut p = config_path.to_path_buf();
        p.set_extension("toml.history");
        let raw = std::fs::read_to_string(&p).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, config_path: &std::path::Path) {
        let mut p = config_path.to_path_buf();
        p.set_extension("toml.history");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&p, json);
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> StoreConfig {
        StoreConfig { enabled: true, db_path: ":memory:".into(), retention_days: 0, ..Default::default() }
    }

    fn test_store() -> impl StoreBackend {
        RusqliteBackend::open(&test_config()).expect("open in-memory store")
    }

    #[test]
    fn insert_and_query() {
        let store = test_store();
        let event = StoredEvent {
            id: None, agent_id: "test-agent".into(), event_type: "task.completed".into(),
            severity: "info".into(), timestamp: 1700000000, metadata_json: r#"{"key":"value"}"#.into(),
        };
        let id = store.insert(&event).expect("insert");
        assert!(id > 0);
        let results = store.query(Some("test-agent"), None, None, Some(10)).expect("query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "test-agent");
    }

    #[test]
    fn count_works() {
        let store = test_store();
        for i in 0..5 {
            store.insert(&StoredEvent {
                id: None, agent_id: format!("agent-{}", i), event_type: "test".into(),
                severity: "info".into(), timestamp: 1700000000 + i, metadata_json: "{}".into(),
            }).unwrap();
        }
        assert_eq!(store.count().unwrap(), 5);
    }

    #[test]
    fn config_defaults() {
        let cfg = StoreConfig::default();
        assert_eq!(cfg.provider, "sqlx");
        assert!(!cfg.enabled);
    }

    #[cfg(feature = "store-nats")]
    #[test]
    fn nats_config_parses() {
        let toml = "enabled = true\nprovider = \"nats\"\nretention_days = 7";
        let cfg: StoreConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.provider, "nats");
    }

    #[test]
    fn sqlx_config_parses() {
        let toml = "enabled = true\nprovider = \"sqlx\"\ndb_path = \":memory:\"";
        let cfg: StoreConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.provider, "sqlx");
    }
}
