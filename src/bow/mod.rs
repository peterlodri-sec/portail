//! BOW — Best Objective World
//!
//! Local encrypted secret store for Portail.
//! AES-256-GCM at rest, argon2id key derivation, CLI-first.

pub mod audit;
pub mod crypto;
pub mod key;

use sqlx::SqlitePool;
use zeroize::Zeroizing;

const KEY_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum BowError {
    #[error("key not found: {0}")]
    NotFound(String),

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("no master key available: {0}")]
    NoMasterKey(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct SecretMeta {
    pub key: String,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

pub struct BowStore {
    pool: SqlitePool,
    key: Zeroizing<[u8; KEY_LEN]>,
}

impl BowStore {
    /// Open or create the BOW database, run migrations.
    pub async fn open(db_path: &str, key: Zeroizing<[u8; KEY_LEN]>) -> Result<Self, BowError> {
        let url = if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("file:{db_path}?mode=rwc")
        };
        let pool = SqlitePool::connect(&url).await?;

        // Run migrations
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bow_secrets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL UNIQUE,
                value BLOB NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            )",
        )
        .execute(&pool)
        .await?;

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
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bow_meta (
                key TEXT PRIMARY KEY,
                value BLOB NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_bow_secrets_key ON bow_secrets(key)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_bow_audit_key ON bow_audit(secret_key)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_bow_audit_ts ON bow_audit(ts)")
            .execute(&pool)
            .await?;

        Ok(Self { pool, key })
    }

    /// Store or update a secret. Returns new version number.
    pub async fn set(&self, key: &str, value: &[u8]) -> Result<u32, BowError> {
        let encrypted = crypto::encrypt(&self.key, value)
            .map_err(|e| BowError::DecryptionFailed(format!("encrypt failed: {e}")))?;

        // Upsert: check if key exists
        let existing: Option<(i32,)> =
            sqlx::query_as("SELECT version FROM bow_secrets WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;

        let new_version = if let Some((ver,)) = existing {
            let new_ver = ver + 1;
            sqlx::query(
                "UPDATE bow_secrets SET value = ?, version = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE key = ?",
            )
            .bind(&encrypted)
            .bind(new_ver)
            .bind(key)
            .execute(&self.pool)
            .await?;
            new_ver as u32
        } else {
            sqlx::query("INSERT INTO bow_secrets (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(&encrypted)
                .execute(&self.pool)
                .await?;
            1
        };

        audit::log_action(&self.pool, key, "set", "cli").await?;
        Ok(new_version)
    }

    /// Decrypt and return a secret.
    pub async fn get(&self, key: &str) -> Result<Vec<u8>, BowError> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as("SELECT value FROM bow_secrets WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        let (encrypted,) = row.ok_or_else(|| BowError::NotFound(key.into()))?;

        let plaintext = crypto::decrypt(&self.key, &encrypted)
            .map_err(|e| BowError::DecryptionFailed(format!("decrypt failed: {e}")))?;

        audit::log_action(&self.pool, key, "get", "cli").await?;
        Ok(plaintext)
    }

    /// List all keys (no decryption).
    pub async fn list(&self) -> Result<Vec<SecretMeta>, BowError> {
        let rows: Vec<(String, i32, String, String)> = sqlx::query_as(
            "SELECT key, version, created_at, updated_at FROM bow_secrets ORDER BY key",
        )
        .fetch_all(&self.pool)
        .await?;

        audit::log_action(&self.pool, "*", "list", "cli").await?;

        Ok(rows
            .into_iter()
            .map(|(key, version, created_at, updated_at)| SecretMeta {
                key,
                version,
                created_at,
                updated_at,
            })
            .collect())
    }

    /// Delete a secret.
    pub async fn delete(&self, key: &str) -> Result<(), BowError> {
        let result = sqlx::query("DELETE FROM bow_secrets WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(BowError::NotFound(key.into()));
        }

        audit::log_action(&self.pool, key, "delete", "cli").await?;
        Ok(())
    }

    /// Re-encrypt a secret with a fresh nonce.
    pub async fn rotate(&self, key: &str) -> Result<u32, BowError> {
        let row: Option<(Vec<u8>, i32)> =
            sqlx::query_as("SELECT value, version FROM bow_secrets WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;

        let (encrypted, version) = row.ok_or_else(|| BowError::NotFound(key.into()))?;

        // Decrypt with old nonce
        let plaintext = crypto::decrypt(&self.key, &encrypted)
            .map_err(|e| BowError::DecryptionFailed(format!("decrypt failed: {e}")))?;

        // Re-encrypt with fresh nonce
        let new_encrypted = crypto::encrypt(&self.key, &plaintext)
            .map_err(|e| BowError::DecryptionFailed(format!("encrypt failed: {e}")))?;

        let new_version = version + 1;
        sqlx::query(
            "UPDATE bow_secrets SET value = ?, version = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE key = ?",
        )
        .bind(&new_encrypted)
        .bind(new_version)
        .bind(key)
        .execute(&self.pool)
        .await?;

        audit::log_action(&self.pool, key, "rotate", "cli").await?;
        Ok(new_version as u32)
    }

    /// Re-encrypt all secrets with a new master key.
    pub async fn rekey(&self, new_key: &[u8; KEY_LEN]) -> Result<(), BowError> {
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as("SELECT key, value FROM bow_secrets")
            .fetch_all(&self.pool)
            .await?;

        let mut tx = self.pool.begin().await?;

        for (key, encrypted) in &rows {
            // Decrypt with old key
            let plaintext = crypto::decrypt(&self.key, encrypted)
                .map_err(|e| BowError::DecryptionFailed(format!("decrypt failed: {e}")))?;

            // Encrypt with new key
            let new_encrypted = crypto::encrypt(new_key, &plaintext)
                .map_err(|e| BowError::DecryptionFailed(format!("encrypt failed: {e}")))?;

            sqlx::query("UPDATE bow_secrets SET value = ? WHERE key = ?")
                .bind(&new_encrypted)
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        audit::log_action(&self.pool, "*", "rekey", "cli").await?;
        Ok(())
    }

    /// Query audit log.
    pub async fn audit(
        &self,
        key_filter: Option<&str>,
    ) -> Result<Vec<audit::AuditEntry>, BowError> {
        Ok(audit::query(&self.pool, key_filter).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new([42u8; KEY_LEN])
    }

    #[tokio::test]
    async fn set_get_delete() {
        let store = BowStore::open(":memory:", test_key()).await.unwrap();

        store.set("test/key", b"hello world").await.unwrap();
        let val = store.get("test/key").await.unwrap();
        assert_eq!(val, b"hello world");

        store.delete("test/key").await.unwrap();
        assert!(matches!(
            store.get("test/key").await,
            Err(BowError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn list_keys() {
        let store = BowStore::open(":memory:", test_key()).await.unwrap();
        store.set("alpha", b"1").await.unwrap();
        store.set("beta", b"2").await.unwrap();

        let keys = store.list().await.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().any(|k| k.key == "alpha"));
        assert!(keys.iter().any(|k| k.key == "beta"));
    }

    #[tokio::test]
    async fn rotate_increments_version() {
        let store = BowStore::open(":memory:", test_key()).await.unwrap();
        store.set("rot", b"v1").await.unwrap();

        let meta = store.list().await.unwrap();
        assert_eq!(meta[0].version, 1);

        store.rotate("rot").await.unwrap();
        let meta = store.list().await.unwrap();
        assert_eq!(meta[0].version, 2);
    }

    #[tokio::test]
    async fn wrong_key_fails_decrypt() {
        let store = BowStore::open(":memory:", test_key()).await.unwrap();
        store.set("secret", b"classified").await.unwrap();

        // Get encrypted data
        let encrypted = store.get("secret").await.unwrap();

        // Try decrypting with wrong key
        let mut wrong_key_data = [0u8; KEY_LEN];
        wrong_key_data[0] = 99;
        let result = crypto::decrypt(&Zeroizing::new(wrong_key_data), &encrypted);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn audit_log_records_actions() {
        let store = BowStore::open(":memory:", test_key()).await.unwrap();
        store.set("aud", b"data").await.unwrap();
        store.get("aud").await.unwrap();

        let entries = store.audit(Some("aud")).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "get");
        assert_eq!(entries[1].action, "set");
    }
}
