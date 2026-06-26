//! Persistent event store — SQLite-backed event log.
//!
//! Replaces the in-memory ring buffer with a durable SQLite database.
//! Supports querying by agent_id, event_type, time range, and exporting
//! to JSON/CSV.
//!
//! # Retention
//!
//! Configured via `retention_days` in `StoreConfig`. A background task
//! purges events older than the retention window every 5 minutes.
//!
//! # Migrations
//!
//! Tables are created on first open via [`EventStore::open`]. No
//! external migration tooling needed.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

// ─── config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: default_db_path(),
            retention_days: default_retention(),
        }
    }
}

fn default_db_path() -> String {
    "/var/lib/portail/events.db".into()
}

fn default_retention() -> u32 {
    30
}

// ─── event model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: Option<i64>,
    pub agent_id: String,
    pub event_type: String,
    pub severity: String,
    pub timestamp: i64,
    pub metadata_json: String,
}

// ─── store ────────────────────────────────────────────────────────

/// Thread-safe SQLite event store.
#[derive(Clone)]
pub struct EventStore {
    db: Arc<Mutex<Connection>>,
    config: StoreConfig,
}

impl EventStore {
    /// Open (or create) the database, run migrations, and spawn retention
    /// cleanup task if retention is configured.
    pub fn open(config: StoreConfig) -> Result<Self, rusqlite::Error> {
        let path = expand_tilde(&config.db_path);
        if let Some(parent) = PathBuf::from(&path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

        // migrations
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
            CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);

            CREATE TABLE IF NOT EXISTS api_keys (
                key         TEXT PRIMARY KEY,
                label       TEXT NOT NULL,
                scopes      TEXT NOT NULL DEFAULT '[]',
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )?;

        let store = Self {
            db: Arc::new(Mutex::new(conn)),
            config,
        };

        // spawn retention cleanup
        if store.config.retention_days > 0 {
            let s = store.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    if let Err(e) = s.purge_expired() {
                        tracing::warn!("event store retention purge failed: {}", e);
                    }
                }
            });
        }

        Ok(store)
    }

    /// Insert a single event.
    pub fn insert(&self, event: &StoredEvent) -> Result<i64, rusqlite::Error> {
        let db = self.db.blocking_lock();
        db.execute(
            "INSERT INTO events (agent_id, event_type, severity, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.agent_id,
                event.event_type,
                event.severity,
                event.timestamp,
                event.metadata_json,
            ],
        )?;
        Ok(db.last_insert_rowid())
    }

    /// Query events with optional filters. Returns newest first.
    pub fn query(
        &self,
        agent_id: Option<&str>,
        event_type: Option<&str>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, rusqlite::Error> {
        let db = self.db.blocking_lock();
        let mut sql = String::from("SELECT id, agent_id, event_type, severity, timestamp, metadata FROM events WHERE 1=1");
        let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(a) = agent_id {
            sql.push_str(" AND agent_id = ?");
            bind_values.push(Box::new(a.to_string()));
        }
        if let Some(t) = event_type {
            sql.push_str(" AND event_type = ?");
            bind_values.push(Box::new(t.to_string()));
        }
        if let Some(s) = since {
            sql.push_str(" AND timestamp >= ?");
            bind_values.push(Box::new(s));
        }
        sql.push_str(" ORDER BY id DESC");
        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT {}", l.min(10000)));
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(StoredEvent {
                id: Some(row.get(0)?),
                agent_id: row.get(1)?,
                event_type: row.get(2)?,
                severity: row.get(3)?,
                timestamp: row.get(4)?,
                metadata_json: row.get(5)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Export events as JSON.
    pub fn export_json(
        &self,
        since: Option<i64>,
    ) -> Result<String, rusqlite::Error> {
        let events = self.query(None, None, since, Some(100000))?;
        serde_json::to_string_pretty(&events).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
        })
    }

    /// Get total count.
    pub fn count(&self) -> Result<i64, rusqlite::Error> {
        let db = self.db.blocking_lock();
        db.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
    }

    /// Delete events older than retention_days.
    fn purge_expired(&self) -> Result<usize, rusqlite::Error> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - (self.config.retention_days as i64 * 86400);

        let db = self.db.blocking_lock();
        let n = db.execute("DELETE FROM events WHERE timestamp < ?1", params![cutoff])?;
        if n > 0 {
            tracing::info!(deleted = n, "event store retention purge");
        }
        Ok(n)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> EventStore {
        EventStore::open(StoreConfig {
            enabled: true,
            db_path: ":memory:".into(),
            retention_days: 0,
        })
        .expect("open in-memory store")
    }

    #[test]
    fn insert_and_query() {
        let store = test_store();
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

        let results = store
            .query(Some("test-agent"), None, None, Some(10))
            .expect("query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "test-agent");
        assert_eq!(results[0].event_type, "task.completed");
    }

    #[test]
    fn count_works() {
        let store = test_store();
        for i in 0..5 {
            store
                .insert(&StoredEvent {
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
}
