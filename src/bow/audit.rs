//! BOW Audit — append-only audit log

use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub secret_key: String,
    pub action: String,
    pub actor: String,
    pub ts: String,
}

/// Write an audit entry.
pub async fn log_action(
    pool: &SqlitePool,
    secret_key: &str,
    action: &str,
    actor: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO bow_audit (secret_key, action, actor) VALUES (?, ?, ?)")
        .bind(secret_key)
        .bind(action)
        .bind(actor)
        .execute(pool)
        .await?;
    Ok(())
}

/// Query audit entries, optionally filtered by key.
pub async fn query(
    pool: &SqlitePool,
    key_filter: Option<&str>,
) -> Result<Vec<AuditEntry>, sqlx::Error> {
    let rows = if let Some(key) = key_filter {
        sqlx::query_as::<_, AuditEntry>(
            "SELECT id, secret_key, action, actor, ts FROM bow_audit WHERE secret_key = ? ORDER BY id DESC LIMIT 100",
        )
        .bind(key)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, AuditEntry>(
            "SELECT id, secret_key, action, actor, ts FROM bow_audit ORDER BY id DESC LIMIT 100",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn audit_roundtrip() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bow_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                secret_key TEXT NOT NULL,
                action TEXT NOT NULL,
                actor TEXT NOT NULL DEFAULT 'cli',
                ts TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        log_action(&pool, "test/key", "set", "cli").await.unwrap();
        log_action(&pool, "test/key", "get", "api").await.unwrap();

        let entries = query(&pool, Some("test/key")).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "get"); // most recent first
        assert_eq!(entries[1].action, "set");

        let all = query(&pool, None).await.unwrap();
        assert_eq!(all.len(), 2);
    }
}
