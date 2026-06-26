//! Schema initialization + sqlx migration runner.
//!
//! Uses sqlx's built-in migration system. All backends call this
//! on startup to ensure the database is at the latest schema version.
//!
//! Migration files live in `src/store/migrations/` and are embedded
//! at compile time via `sqlx::migrate!()`.

use std::path::PathBuf;

/// Run all pending migrations against the given database pool.
/// Embedded at compile time — no runtime file I/O needed.
pub async fn run_migrations(pool: &sqlx::SqlitePool) -> Result<(), String> {
    sqlx::migrate!("src/store/migrations")
        .run(pool)
        .await
        .map_err(|e| format!("store migration failed: {}", e))
}

/// Create a new SQLite pool with WAL mode and performance optimizations.
pub async fn create_pool(db_path: &str) -> Result<sqlx::SqlitePool, String> {
    let path = super::expand_tilde(db_path);
    if let Some(parent) = PathBuf::from(&path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let url = format!("sqlite:{}?mode=rwc", &path);
    sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .min_connections(1)
        .connect(&url)
        .await
        .map_err(|e| format!("failed to open store: {}", e))
}

/// Initialize the store: create pool, run migrations, apply pragmas.
pub async fn init(config: &super::StoreConfig) -> Result<sqlx::SqlitePool, String> {
    let expanded = super::expand_tilde(&config.db_path);
    let pool = create_pool(&config.db_path).await?;

    // Performance pragmas (accept dynamic SQL for configuration tuning)
    let pragmas: [&str; 6] = [
        "PRAGMA journal_mode=WAL",
        "PRAGMA synchronous=NORMAL",
        "PRAGMA cache_size=-65536",
        "PRAGMA busy_timeout=5000",
        "PRAGMA foreign_keys=ON",
        "PRAGMA mmap_size=268435456",
    ];
    for pragma in &pragmas {
        let mut builder = sqlx::QueryBuilder::new(*pragma);
        builder.build().execute(&pool).await.ok();
    }

    tracing::info!(path = %expanded, "store pool created, running migrations");
    run_migrations(&pool).await?;

    Ok(pool)
}
