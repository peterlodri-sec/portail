CREATE TABLE IF NOT EXISTS events (
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
