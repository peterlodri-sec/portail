//! Self-healing config — file watcher, auto-reload, validation, versioning.
//!
//! # v1.1
//!
//! Watches the portail.toml file for changes using filesystem polling.
//! On change: loads, validates, applies. If invalid, keeps the old config
//! and logs the error. Stores a rolling history of valid configs for rollback.

use crate::config::Config;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

/// Rolling history of last N valid configs.
const MAX_HISTORY: usize = 5;

fn history_path(config_path: &std::path::Path) -> PathBuf {
    let mut p = config_path.to_path_buf();
    p.set_extension("toml.history");
    p
}

/// Persisted version history (for offline CLI access).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedHistory {
    pub versions: Vec<PersistedVersion>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedVersion {
    pub version: u64,
    pub loaded_at: String,
    pub config_json: String,
}

impl PersistedHistory {
    pub fn load(config_path: &std::path::Path) -> Option<Self> {
        let path = history_path(config_path);
        let raw = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn save(&self, config_path: &std::path::Path) {
        let path = history_path(config_path);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigVersion {
    pub config: Config,
    pub loaded_at: SystemTime,
    pub version: u64,
}

pub struct ConfigWatcher {
    pub path: PathBuf,
    pub health: AtomicBool,
    pub last_error: RwLock<Option<String>>,
    history: RwLock<Vec<ConfigVersion>>,
    version_counter: AtomicU64,
    last_mtime: RwLock<Option<SystemTime>>,
}

impl ConfigWatcher {
    pub fn new(path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            path,
            health: AtomicBool::new(true),
            last_error: RwLock::new(None),
            history: RwLock::new(Vec::with_capacity(MAX_HISTORY)),
            version_counter: AtomicU64::new(0),
            last_mtime: RwLock::new(None),
        })
    }

    /// Load initial config and record its mtime.
    pub async fn init(&self) -> Result<Config, anyhow::Error> {
        let cfg = Config::load(Some(&self.path))?;
        self.record_mtime(&self.path).await;
        self.store_version(cfg.clone()).await;
        Ok(cfg)
    }

    /// Poll for changes. Returns Some(new_config) if file changed and loaded ok.
    pub async fn poll(&self) -> Option<Config> {
        let current_mtime = match self.file_mtime().await {
            Some(t) => t,
            None => return None, // file missing, skip
        };

        let last = *self.last_mtime.read().await;
        if Some(current_mtime) == last {
            return None; // no change
        }

        // File changed — try to load
        match Config::load(Some(&self.path)) {
            Ok(new_cfg) => {
                *self.last_mtime.write().await = Some(current_mtime);
                self.health.store(true, Ordering::Release);
                *self.last_error.write().await = None;
                self.store_version(new_cfg.clone()).await;
                tracing::info!(path = %self.path.display(), "config auto-reloaded");
                Some(new_cfg)
            }
            Err(e) => {
                self.health.store(false, Ordering::Release);
                *self.last_error.write().await = Some(e.to_string());
                tracing::error!(path = %self.path.display(), error = %e, "config reload failed — keeping old config");
                *self.last_mtime.write().await = Some(current_mtime); // record attempt so we don't spam
                None
            }
        }
    }

    /// Store a valid config in the rolling history.
    async fn store_version(&self, config: Config) {
        let mut history = self.history.write().await;
        let version = self.version_counter.fetch_add(1, Ordering::Relaxed) + 1;
        history.push(ConfigVersion {
            config: config.clone(),
            loaded_at: SystemTime::now(),
            version,
        });
        if history.len() > MAX_HISTORY {
            history.remove(0);
        }
        // Persist to sidecar file for offline CLI access
        let now = chrono::Utc::now().to_rfc3339();
        if let Ok(json) = serde_json::to_string(&config) {
            let mut persisted = PersistedHistory::load(&self.path).unwrap_or(PersistedHistory { versions: Vec::new() });
            persisted.versions.push(PersistedVersion { version, loaded_at: now, config_json: json });
            if persisted.versions.len() > MAX_HISTORY {
                persisted.versions.remove(0);
            }
            persisted.save(&self.path);
        }
    }

    /// Return the most recent version for rollback display.
    pub async fn current_version(&self) -> u64 {
        self.version_counter.load(Ordering::Relaxed)
    }

    /// Rollback to a previous version (1-indexed). Returns None if version not found.
    pub async fn rollback(&self, version: u64) -> Option<Config> {
        let history = self.history.read().await;
        history.iter()
            .find(|v| v.version == version)
            .map(|v| v.config.clone())
    }

    /// List available config versions for rollback.
    pub async fn version_history(&self) -> Vec<ConfigVersion> {
        self.history.read().await.clone()
    }

    async fn record_mtime(&self, path: &std::path::Path) {
        *self.last_mtime.write().await = file_mtime(path).await;
    }

    async fn file_mtime(&self) -> Option<SystemTime> {
        file_mtime(&self.path).await
    }

    pub fn is_healthy(&self) -> bool {
        self.health.load(Ordering::Acquire)
    }
}

async fn file_mtime(path: &std::path::Path) -> Option<SystemTime> {
    tokio::fs::metadata(path).await.ok()?.modified().ok()
}

/// Spawn the watcher background task. Applies valid configs to AppState.
pub async fn spawn_watcher(
    watcher: Arc<ConfigWatcher>,
    state: Arc<crate::AppState>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Some(new_cfg) = watcher.poll().await {
                *state.config.write().unwrap() = new_cfg;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path() -> PathBuf {
        let dir = std::env::temp_dir();
        dir.join(format!("portail-test-{}.toml", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn poll_detects_change() {
        let path = temp_config_path();
        let toml = r#"listen = "127.0.0.1:9999""#;
        std::fs::write(&path, toml).unwrap();

        let watcher = ConfigWatcher::new(path.clone());
        let cfg = watcher.init().await.unwrap();
        assert_eq!(cfg.listen, "127.0.0.1:9999");
        // Need a pause so mtime advances
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert!(watcher.poll().await.is_none());

        // Write a new config
        let toml2 = r#"listen = "0.0.0.0:8080""#;
        std::fs::write(&path, toml2).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let updated = watcher.poll().await;
        assert!(updated.is_some(), "expected poll to detect change");
        assert_eq!(updated.unwrap().listen, "0.0.0.0:8080");

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn poll_ignores_invalid_config() {
        let path = temp_config_path();
        std::fs::write(&path, "listen = \"127.0.0.1:9999\"").unwrap();

        let watcher = ConfigWatcher::new(path.clone());
        let _ = watcher.init().await.unwrap();
        assert!(watcher.is_healthy());

        // Write garbage
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::fs::write(&path, "{{{ not toml }}}").unwrap();

        let result = watcher.poll().await;
        assert!(result.is_none());
        assert!(!watcher.is_healthy());
        assert!(watcher.last_error.read().await.is_some());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn version_history_rolls() {
        let path = temp_config_path();
        for i in 0..7 {
            let toml = format!(r#"listen = "0.0.0.0:{}""#, 8000 + i);
            std::fs::write(&path, &toml).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let watcher = ConfigWatcher::new(path.clone());
            if i == 0 {
                let _ = watcher.init().await.unwrap();
            }
            if i > 0 {
                let _ = watcher.poll().await;
            }
        }
        let _ = std::fs::remove_file(&path);
        let _watcher = ConfigWatcher::new(path.clone());
        let history_path = history_path(&path);
        assert!(history_path.exists());
    }

    #[tokio::test]
    async fn rollback_returns_correct_version() {
        let path = temp_config_path();
        std::fs::write(&path, "listen = \"0.0.0.0:8001\"").unwrap();

        let watcher = ConfigWatcher::new(path.clone());
        let _ = watcher.init().await.unwrap();
        assert_eq!(watcher.current_version().await, 1);

        // Write second version
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::fs::write(&path, "listen = \"0.0.0.0:8002\"").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = watcher.poll().await;
        assert_eq!(watcher.current_version().await, 2);

        let rolled = watcher.rollback(1).await;
        assert!(rolled.is_some());
        assert_eq!(rolled.unwrap().listen, "0.0.0.0:8001");

        // Nonexistent version
        assert!(watcher.rollback(999).await.is_none());

        let _ = std::fs::remove_file(&path);
    }
}
