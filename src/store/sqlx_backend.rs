//! sqlx backend — async, compile-time checked SQLite queries.
//!
//! Uses `sqlx::SqlitePool` for async I/O. Queries are validated
//! at compile time when using `query!` macro (requires DATABASE_URL).
//! Fallback: `sqlx::query()` with bind parameters.

use std::time::{SystemTime, UNIX_EPOCH};

use super::expand_tilde;
use super::queries;
use super::{StoreBackend, StoreConfig, StoredEvent};

pub struct SqlxBackend {
    pub pool: sqlx::SqlitePool,
}

impl SqlxBackend {
    pub async fn open(config: &StoreConfig) -> Result<Self, String> {
        let path = expand_tilde(&config.db_path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let url = format!("sqlite:{}?mode=rwc", path);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| e.to_string())?;

        sqlx::query(queries::MIGRATION_SQL)
            .execute(&pool)
            .await
            .ok();
        sqlx::query(queries::CREATE_EVENTS_TABLE)
            .execute(&pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query(queries::CREATE_INDEXES)
            .execute(&pool)
            .await
            .ok();

        Ok(Self { pool })
    }
}

impl StoreBackend for SqlxBackend {
    fn insert(&self, event: &StoredEvent) -> Result<i64, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let result = sqlx::query(queries::INSERT_EVENT_RETURNING)
                .bind(event.agent_id.as_str())
                .bind(event.event_type.as_str())
                .bind(event.severity.as_str())
                .bind(event.timestamp)
                .bind(event.metadata_json.as_str())
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.last_insert_rowid())
        })
    }

    fn query(
        &self,
        agent_id: Option<&str>,
        event_type: Option<&str>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let mut builder = sqlx::QueryBuilder::new(super::queries::SELECT_EVENTS_BASE);
            if let Some(a) = agent_id {
                builder.push(" AND agent_id = ").push_bind(a);
            }
            if let Some(e) = event_type {
                builder.push(" AND event_type = ").push_bind(e);
            }
            if let Some(s) = since {
                builder.push(" AND timestamp >= ").push_bind(s);
            }
            builder.push(" ORDER BY id DESC");
            if let Some(l) = limit {
                builder.push(format!(" LIMIT {}", l.min(10000)));
            }

            let rows = builder
                .build_query_as::<SqlxEventRow>()
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.into()).collect())
        })
    }

    fn count(&self) -> Result<i64, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let (count,): (i64,) = sqlx::query_as(queries::COUNT_EVENTS)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
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
            let result = sqlx::query(queries::DELETE_EXPIRED)
                .bind(cutoff)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.rows_affected() as usize)
        })
    }

    fn export_json(&self, since: Option<i64>) -> Result<String, String> {
        self.query(None, None, since, Some(100000))
            .and_then(|e| serde_json::to_string_pretty(&e).map_err(|e| e.to_string()))
    }
}

#[derive(sqlx::FromRow)]
struct SqlxEventRow {
    id: i64,
    agent_id: String,
    event_type: String,
    severity: String,
    timestamp: i64,
    metadata: String,
}

impl From<SqlxEventRow> for StoredEvent {
    fn from(r: SqlxEventRow) -> Self {
        StoredEvent {
            id: Some(r.id),
            agent_id: r.agent_id,
            event_type: r.event_type,
            severity: r.severity,
            timestamp: r.timestamp,
            metadata_json: r.metadata,
        }
    }
}
