//! Persistent event store — pluggable backends (SQLite / Turso / libSQL).
//!
//! # v2.0
//!
//! Multiple backends behind a `StoreBackend` trait. SQLite is the default.
//! Turso (libSQL) is available via the `store-turso` Cargo feature.
//!
//! # Backend selection
//!
//! ```toml
//! [store]
//! enabled = true
//! provider = "turso"  # "sqlite" (default) | "turso"
//! turso_url = "libsql://my-db.turso.io"
//! turso_auth_token = "$TURSO_TOKEN"
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    /// Backend provider: "sqlite" (default) or "turso"
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Turso: libSQL URL (only when provider = "turso")
    #[serde(default)]
    pub turso_url: Option<String>,
    /// Turso: auth token (only when provider = "turso")
    #[serde(default)]
    pub turso_auth_token: Option<String>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: default_db_path(),
            retention_days: default_retention(),
            provider: default_provider(),
            turso_url: None,
            turso_auth_token: None,
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

// ─── Turso Backend (feature-gated) ─────────────────────────────────

#[cfg(feature = "store-turso")]
pub struct TursoBackend {
    db: libsql::Database,
}

#[cfg(feature = "store-turso")]
impl TursoBackend {
    pub async fn open(config: &StoreConfig) -> Result<Self, String> {
        let url = config
            .turso_url
            .as_deref()
            .unwrap_or("libsql://localhost:8080");
        let token = config.turso_auth_token.as_deref().unwrap_or("");
        let db = libsql::Builder::new_remote(url.to_string(), token.to_string())
            .build()
            .await
            .map_err(|e| e.to_string())?;
        let conn = db.connect().map_err(|e| e.to_string())?;
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
        .await
        .map_err(|e| e.to_string())?;
        Ok(Self { db })
    }
}

#[cfg(feature = "store-turso")]
impl StoreBackend for TursoBackend {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let conn = self.db.connect().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "INSERT INTO events (agent_id, event_type, severity, timestamp, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
                )
                .await
                .map_err(|e| e.to_string())?;
            let mut rows = stmt
                .query([
                    event.agent_id.clone(),
                    event.event_type.clone(),
                    event.severity.clone(),
                    event.timestamp.to_string(),
                    event.metadata_json.clone(),
                ])
                .await
                .map_err(|e| e.to_string())?;
            let id: i64 = rows
                .next()
                .await
                .map_err(|e| e.to_string())?
                .and_then(|r| r.get::<i64>(0).ok())
                .unwrap_or(0);
            Ok(id)
        })
    }

    fn query(
        &self,
        _agent_id: Option<&str>,
        _event_type: Option<&str>,
        _since: Option<i64>,
        _limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String> {
        // TODO: full query implementation with params
        Ok(Vec::new())
    }

    fn count(&self) -> Result<i64, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let conn = self.db.connect().map_err(|e| e.to_string())?;
            let mut rows = conn.query("SELECT COUNT(*) FROM events", ()).await.map_err(|e| e.to_string())?;
            let count: i64 = rows.next().await.map_err(|e| e.to_string())?.map(|r| r.get(0).unwrap_or(0)).unwrap_or(0);
            Ok(count)
        })
    }

    fn purge_expired(&self, retention_days: u32) -> Result<usize, String> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - (retention_days as i64 * 86400);
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let conn = self.db.connect().map_err(|e| e.to_string())?;
            let n = conn
                .execute("DELETE FROM events WHERE timestamp < ?1", [cutoff.to_string()])
                .await
                .map_err(|e| e.to_string())?;
            Ok(n as usize)
        })
    }

    fn export_json(&self, since: Option<i64>) -> Result<String, String> {
        self.query(None, None, since, Some(100000))
            .and_then(|events| serde_json::to_string_pretty(&events).map_err(|e| e.to_string()))
    }
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
            #[cfg(feature = "store-turso")]
            "turso" => {
                tracing::info!("opening Turso event store at {}", config.turso_url.as_deref().unwrap_or("localhost"));
                let rt = tokio::runtime::Handle::current();
                let turso = rt.block_on(TursoBackend::open(&config))?;
                Arc::new(turso)
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

    #[cfg(feature = "store-turso")]
    #[test]
    fn turso_config_parses() {
        let toml = r#"
            enabled = true
            provider = "turso"
            turso_url = "libsql://test-db.turso.io"
            turso_auth_token = "secret"
            retention_days = 7
        "#;
        let cfg: StoreConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.provider, "turso");
        assert_eq!(cfg.turso_url, Some("libsql://test-db.turso.io".into()));
    }
}
