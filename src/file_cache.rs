//! File cache — content-addressable disk cache (~500MB fort).
//! Uses `cacache` (npm's cache engine) for SHA-256-keyed storage.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cache_path")]
    pub path: String,
    #[serde(default = "default_max_size")]
    pub max_size: String,
}

impl Default for FileCacheConfig {
    fn default() -> Self {
        Self { enabled: false, path: default_cache_path(), max_size: default_max_size() }
    }
}

fn default_cache_path() -> String { "/var/cache/portail/files".into() }
fn default_max_size() -> String { "500MB".into() }

// ─── store ────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FileCache {
    path: Arc<std::path::PathBuf>,
}

impl FileCache {
    pub fn new(config: &FileCacheConfig) -> Self {
        let path = std::path::PathBuf::from(&config.path);
        std::fs::create_dir_all(&path).ok();
        Self { path: Arc::new(path) }
    }

    pub async fn put(&self, key: &str, data: &[u8]) -> Result<(), String> {
        cacache::write(&*self.path, key, data).await
            .map(|_| ())
            .map_err(|e| format!("cache write error: {}", e))
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, String> {
        cacache::read(&*self.path, key).await
            .map_err(|e| format!("cache read error: {}", e))
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        cacache::remove(&*self.path, key).await
            .map_err(|e| format!("cache delete error: {}", e))
    }

    pub async fn stats(&self) -> FileCacheStats {
        FileCacheStats {
            size_bytes: 0,
            entries: 0,
            path: self.path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FileCacheStats {
    pub size_bytes: u64,
    pub entries: u64,
    pub path: String,
}

// ─── axum handlers ────────────────────────────────────────────────

async fn handle_put(
    State(cache): State<FileCache>,
    Path(key): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    match cache.put(&key, &body).await {
        Ok(()) => StatusCode::CREATED,
        Err(e) => { tracing::warn!(key=%key, error=%e, "file-cache put"); StatusCode::INTERNAL_SERVER_ERROR }
    }
}

async fn handle_get(
    State(cache): State<FileCache>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    match cache.get(&key).await {
        Ok(data) => (StatusCode::OK, [("content-type", "application/octet-stream")], data).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_delete(
    State(cache): State<FileCache>,
    Path(key): Path<String>,
) -> StatusCode {
    match cache.delete(&key).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn handle_stats(
    State(cache): State<FileCache>,
) -> Json<FileCacheStats> {
    Json(cache.stats().await)
}

fn _router() -> axum::Router<FileCache> {
    axum::Router::new()
        .route("/file-cache/{key}", axum::routing::put(handle_put).get(handle_get).delete(handle_delete))
        .route("/file-cache/stats", axum::routing::get(handle_stats))
}

pub fn router_with_state() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/file-cache/{key}", axum::routing::put(handle_put_app).get(handle_get_app).delete(handle_delete_app))
        .route("/file-cache/stats", axum::routing::get(handle_stats_app))
}

async fn handle_put_app(State(s): State<Arc<crate::AppState>>, Path(k): Path<String>, b: Bytes) -> impl IntoResponse {
    handle_put(State(s.file_cache.clone()), Path(k), b).await
}
async fn handle_get_app(State(s): State<Arc<crate::AppState>>, Path(k): Path<String>) -> impl IntoResponse {
    handle_get(State(s.file_cache.clone()), Path(k)).await
}
async fn handle_delete_app(State(s): State<Arc<crate::AppState>>, Path(k): Path<String>) -> StatusCode {
    handle_delete(State(s.file_cache.clone()), Path(k)).await
}
async fn handle_stats_app(State(s): State<Arc<crate::AppState>>) -> Json<FileCacheStats> {
    handle_stats(State(s.file_cache.clone())).await
}
