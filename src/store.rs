//! Persistent event store — pluggable backends (SQLite / NATS).
//!
//! # v2.0
//!
//! - SQLite: single-node, zero-config. Always available.
//! - NATS-replicated: multi-node via NATS pub/sub. Free, open source.
//!   Enable with `store-nats` Cargo feature.
//!
//! # Configuration
//!
//! ```toml
//! [store]
//! enabled = true
//! provider = "sqlite"  # "sqlite" (default) | "nats"
//! db_path = "/var/lib/portail/events.db"
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "store-nats")]
use futures::StreamExt;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

// ─── config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoreConfig {
    /// Enable persistent event store
    #[serde(default)]
    pub enabled: bool,
    /// Path to SQLite database file
    #[serde(default = "default_db_path")]
    pub db_path: String,
    /// Retention in days (0 = unlimited)
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    /// Backend provider: "sqlite" (default) or "nats"
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
fn default_provider() -> String { "sqlite".into() }

// ─── event model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredEvent {
    pub id: Option<i64>,
    pub agent_id: String,
    pub event_type: String,
    pub severity: String,
    pub timestamp: i64,
    pub metadata_json: String,
}

// ─── StoreBackend trait (v2.0 abstraction) ────────────────────────

/// Backend-agnostic event store interface.
///
/// Implementations: [`SqliteBackend`], `TursoBackend` (feature-gated).
pub trait StoreBackend: Send + Sync + 'static {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String>;
    fn query(
        &self,
        agent_id: Option<&str>,
        event_type: Option<&str>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String>;
    fn count(&self) -> Result<i64, String>;
    fn purge_expired(&self, retention_days: u32) -> Result<usize, String>;
    fn export_json(&self, since: Option<i64>) -> Result<String, String>;
}

// ─── SQLite Backend (default) ─────────────────────────────────────

pub struct SqliteBackend {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteBackend {
    pub fn open(config: &StoreConfig) -> Result<Self, String> {
        let path = expand_tilde(&config.db_path);
        if let Some(parent) = PathBuf::from(&path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id    TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                severity    TEXT NOT NULL DEFAULT 'info',
                timestamp   INTEGER NOT NULL,
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE INDEX IF NOT EXISTS idx_events_agent   ON events(agent_id);
            CREATE INDEX IF NOT EXISTS idx_events_type    ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_events_ts      ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { db: Arc::new(Mutex::new(conn)) })
    }
}

impl StoreBackend for SqliteBackend {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        let db = self.db.blocking_lock();
        db.execute(
            "INSERT INTO events (agent_id, event_type, severity, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![event.agent_id, event.event_type, event.severity, event.timestamp, event.metadata_json],
        )
        .map_err(|e| e.to_string())?;
        Ok(db.last_insert_rowid())
    }

    fn query(
        &self,
        agent_id: Option<&str>,
        event_type: Option<&str>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String> {
        let db = self.db.blocking_lock();
        let mut sql = String::from("SELECT id, agent_id, event_type, severity, timestamp, metadata FROM events WHERE 1=1");
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(a) = agent_id {
            sql.push_str(" AND agent_id = ?");
            values.push(Box::new(a.to_string()));
        }
        if let Some(t) = event_type {
            sql.push_str(" AND event_type = ?");
            values.push(Box::new(t.to_string()));
        }
        if let Some(s) = since {
            sql.push_str(" AND timestamp >= ?");
            values.push(Box::new(s));
        }
        sql.push_str(" ORDER BY id DESC");
        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT {}", l.min(10000)));
        }

        let refs: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(refs.as_slice(), |row| {
            Ok(StoredEvent {
                id: Some(row.get(0)?),
                agent_id: row.get(1)?,
                event_type: row.get(2)?,
                severity: row.get(3)?,
                timestamp: row.get(4)?,
                metadata_json: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| e.to_string())?);
        }
        Ok(events)
    }

    fn count(&self) -> Result<i64, String> {
        let db = self.db.blocking_lock();
        db.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .map_err(|e| e.to_string())
    }

    fn purge_expired(&self, retention_days: u32) -> Result<usize, String> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - (retention_days as i64 * 86400);
        let db = self.db.blocking_lock();
        let n = db
            .execute("DELETE FROM events WHERE timestamp < ?1", rusqlite::params![cutoff])
            .map_err(|e| e.to_string())?;
        if n > 0 {
            tracing::info!(deleted = n, "event store retention purge");
        }
        Ok(n)
    }

    fn export_json(&self, since: Option<i64>) -> Result<String, String> {
        let events = self.query(None, None, since, Some(100000))?;
        serde_json::to_string_pretty(&events).map_err(|e| e.to_string())
    }
}

// ─── NATS-Replicated Backend (feature-gated, free/open source) ────

#[cfg(feature = "store-nats")]
pub struct NatsReplicatedBackend {
    local: SqliteBackend,
    nc: async_nats::Client,
}

#[cfg(feature = "store-nats")]
impl NatsReplicatedBackend {
    pub async fn open(config: &StoreConfig) -> Result<Self, String> {
        let local = SqliteBackend::open(config)?;
        let nats_url = std::env::var("PORTAIL_NATS_URL")
            .unwrap_or_else(|_| "nats://localhost:4222".into());
        let nc = async_nats::connect(&nats_url).await.map_err(|e| e.to_string())?;

        let db = local.db.clone();
        let sub_nc = nc.clone();
        tokio::spawn(async move {
            let mut sub = sub_nc.subscribe("portail.store.events".to_string()).await
                .expect("NATS subscribe for store replication");
            while let Some(msg) = sub.next().await {
                if let Ok(event) = serde_json::from_slice::<StoredEvent>(&msg.payload) {
                    let conn = db.blocking_lock();
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO events (agent_id, event_type, severity, timestamp, metadata)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![event.agent_id, event.event_type, event.severity, event.timestamp, event.metadata_json],
                    );
                }
            }
        });

        Ok(Self { local, nc })
    }

    fn publish_to_nats(&self, event: &StoredEvent) {
        let nc = self.nc.clone();
        let payload = serde_json::to_vec(event).unwrap_or_default();
        tokio::spawn(async move {
            let _ = nc.publish("portail.store.events".to_string(), payload.into()).await;
        });
    }
}

#[cfg(feature = "store-nats")]
impl StoreBackend for NatsReplicatedBackend {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        let id = self.local.insert(event)?;
        self.publish_to_nats(event);
        Ok(id)
    }
    fn query(&self, a: Option<&str>, e: Option<&str>, s: Option<i64>, l: Option<usize>)
        -> Result<Vec<StoredEvent>, String> { self.local.query(a, e, s, l) }
    fn count(&self) -> Result<i64, String> { self.local.count() }
    fn purge_expired(&self, d: u32) -> Result<usize, String> { self.local.purge_expired(d) }
    fn export_json(&self, s: Option<i64>) -> Result<String, String> { self.local.export_json(s) }
}

// ─── EventStore (facade) ──────────────────────────────────────────

/// Thread-safe event store with pluggable backend.
#[derive(Clone)]
pub struct EventStore {
    backend: Arc<dyn StoreBackend>,
    config: StoreConfig,
}

impl EventStore {
    /// Open the store with the configured backend.
    pub fn open(config: StoreConfig) -> Result<Self, String> {
        let backend: Arc<dyn StoreBackend> = match config.provider.as_str() {
            #[cfg(feature = "store-nats")]
            "nats" => {
                tracing::info!("opening NATS-replicated event store");
                let rt = tokio::runtime::Handle::current();
                let nats = rt.block_on(NatsReplicatedBackend::open(&config))?;
                Arc::new(nats)
            }
            _ => {
                tracing::info!(path = %config.db_path, "opening SQLite event store");
                Arc::new(SqliteBackend::open(&config)?)
            }
        };

        let store = Self { backend: backend.clone(), config: config.clone() };

        // spawn retention
        if store.config.retention_days > 0 {
            let s = store.clone();
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

        Ok(store)
    }

    pub fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        self.backend.insert(event)
    }

    pub fn query(
        &self,
        agent_id: Option<&str>,
        event_type: Option<&str>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String> {
        self.backend.query(agent_id, event_type, since, limit)
    }

    pub fn count(&self) -> Result<i64, String> {
        self.backend.count()
    }

    pub fn export_json(&self, since: Option<i64>) -> Result<String, String> {
        self.backend.export_json(since)
    }
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.to_string_lossy().to_string() + &path[1..];
        }
    }
    path.to_string()
}

// ─── tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite_store() -> EventStore {
        EventStore::open(StoreConfig {
            enabled: true,
            db_path: ":memory:".into(),
            retention_days: 0,
            ..Default::default()
        })
        .expect("open in-memory store")
    }

    #[test]
    fn insert_and_query() {
        let store = sqlite_store();
        let event = StoredEvent {
            id: None,
            agent_id: "test-agent".into(),
            event_type: "task.completed".into(),
            severity: "info".into(),
            timestamp: 1700000000,
            metadata_json: r#"{"key":"value"}"#.into(),
        };
        let id = store.insert(&event).expect("insert");
        assert!(id > 0);

        let results = store.query(Some("test-agent"), None, None, Some(10)).expect("query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "test-agent");
    }

    #[test]
    fn count_works() {
        let store = sqlite_store();
        for i in 0..5 {
            store.insert(&StoredEvent {
                id: None,
                agent_id: format!("agent-{}", i),
                event_type: "test".into(),
                severity: "info".into(),
                timestamp: 1700000000 + i,
                metadata_json: "{}".into(),
            })
            .unwrap();
        }
        assert_eq!(store.count().unwrap(), 5);
    }

    #[test]
    fn config_defaults_to_sqlite() {
        let cfg = StoreConfig::default();
        assert_eq!(cfg.provider, "sqlite");
        assert!(!cfg.enabled);
    }

    #[cfg(feature = "store-nats")]
    #[test]
    fn nats_config_parses() {
        let toml = r#"
            enabled = true
            provider = "nats"
            retention_days = 7
        "#;
        let cfg: StoreConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.provider, "nats");
    }
}
