//! Shared SQL queries — used by all store backends.
//!
//! Centralizing queries ensures consistency across rusqlite, sqlx, and NATS backends.
//! All DDL and DML statements live here.

/// Create the events table and indexes.
pub const CREATE_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id    TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    severity    TEXT NOT NULL DEFAULT 'info',
    timestamp   INTEGER NOT NULL,
    metadata    TEXT NOT NULL DEFAULT '{}',
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
"#;

/// Create indexes for common queries.
pub const CREATE_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_events_agent   ON events(agent_id);
CREATE INDEX IF NOT EXISTS idx_events_type    ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_ts      ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
"#;

/// Full migration SQL (table + indexes + pragma).
pub const MIGRATION_SQL: &str = r#"
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
"#;

/// Combined DDL for fresh setup.
pub fn setup_sql() -> String {
    format!(
        "{}\n{}\n{}",
        MIGRATION_SQL, CREATE_EVENTS_TABLE, CREATE_INDEXES
    )
}

/// Insert a single event. Parameters: agent_id, event_type, severity, timestamp, metadata.
pub const INSERT_EVENT: &str = r#"
INSERT INTO events (agent_id, event_type, severity, timestamp, metadata)
VALUES (?1, ?2, ?3, ?4, ?5)
"#;

/// Insert with RETURNING id (sqlx-compatible, SQLite 3.35+).
pub const INSERT_EVENT_RETURNING: &str = r#"
INSERT INTO events (agent_id, event_type, severity, timestamp, metadata)
VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id
"#;

/// Count all events.
pub const COUNT_EVENTS: &str = "SELECT COUNT(*) FROM events";

/// Delete events older than cutoff timestamp.
pub const DELETE_EXPIRED: &str = "DELETE FROM events WHERE timestamp < ?1";

/// Select events with optional filters. Appends WHERE clauses dynamically.
pub const SELECT_EVENTS_BASE: &str = r#"
SELECT id, agent_id, event_type, severity, timestamp, metadata
FROM events WHERE 1=1
"#;

/// Insert-or-ignore (used by NATS replication to avoid duplicates).
pub const INSERT_OR_IGNORE_EVENT: &str = r#"
INSERT OR IGNORE INTO events (agent_id, event_type, severity, timestamp, metadata)
VALUES (?1, ?2, ?3, ?4, ?5)
"#;
