//! Rusqlite backend — synchronous SQLite (for tests and backwards compat).
//!
//! Uses shared queries from `super::queries`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::queries;
use super::{StoreBackend, StoreConfig, StoredEvent};

pub struct RusqliteBackend {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl RusqliteBackend {
    pub fn open(config: &StoreConfig) -> Result<Self, String> {
        let path = super::expand_tilde(&config.db_path);
        if let Some(parent) = PathBuf::from(&path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute_batch(&queries::setup_sql()).map_err(|e| e.to_string())?;
        Ok(Self { db: Arc::new(Mutex::new(conn)) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl StoreBackend for RusqliteBackend {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        let db = self.lock();
        db.execute(
            queries::INSERT_EVENT,
            rusqlite::params![event.agent_id, event.event_type, event.severity, event.timestamp, event.metadata_json],
        ).map_err(|e| e.to_string())?;
        Ok(db.last_insert_rowid())
    }

    fn query(
        &self, agent_id: Option<&str>, event_type: Option<&str>,
        since: Option<i64>, limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String> {
        let db = self.lock();
        let mut sql = String::from(queries::SELECT_EVENTS_BASE);
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(a) = agent_id { sql.push_str(" AND agent_id = ?"); values.push(Box::new(a.to_string())); }
        if let Some(t) = event_type { sql.push_str(" AND event_type = ?"); values.push(Box::new(t.to_string())); }
        if let Some(s) = since { sql.push_str(" AND timestamp >= ?"); values.push(Box::new(s)); }
        sql.push_str(" ORDER BY id DESC");
        if let Some(l) = limit { sql.push_str(&format!(" LIMIT {}", l.min(10000))); }
        let refs: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(refs.as_slice(), |row| {
            Ok(StoredEvent { id: Some(row.get(0)?), agent_id: row.get(1)?, event_type: row.get(2)?, severity: row.get(3)?, timestamp: row.get(4)?, metadata_json: row.get(5)? })
        }).map_err(|e| e.to_string())?;
        let mut events = Vec::new();
        for row in rows { events.push(row.map_err(|e| e.to_string())?); }
        Ok(events)
    }

    fn count(&self) -> Result<i64, String> {
        self.lock().query_row(queries::COUNT_EVENTS, [], |r| r.get(0)).map_err(|e| e.to_string())
    }

    fn purge_expired(&self, retention_days: u32) -> Result<usize, String> {
        let cutoff = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 - (retention_days as i64 * 86400);
        let n = self.lock().execute(queries::DELETE_EXPIRED, rusqlite::params![cutoff]).map_err(|e| e.to_string())?;
        if n > 0 { tracing::info!(deleted = n, "event store retention purge"); }
        Ok(n)
    }

    fn export_json(&self, since: Option<i64>) -> Result<String, String> {
        let events = self.query(None, None, since, Some(100000))?;
        serde_json::to_string_pretty(&events).map_err(|e| e.to_string())
    }
}
