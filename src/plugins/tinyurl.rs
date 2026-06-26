/*
 * Auto-TinyURL Plugin
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                  Auto-TinyURL Flow                         │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Long URL (internal network)                               │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  Generate  │────▶│  Store in  │────▶│  Return    │     │
 *   │   │  Short ID  │     │  HashMap   │     │  Short URL │     │
 *   │   └────────────┘     └────────────┘     └────────────┘     │
 *   │        │                                                    │
 *   │        │ base62(hash(url + secret))                        │
 *   │        │                                                    │
 *   │   Short URL: http://portail:8787/s/abc123                  │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  Lookup    │────▶│  Get       │────▶│  301       │     │
 *   │   │  by ID     │     │  Original  │     │  Redirect  │     │
 *   │   └────────────┘     └────────────┘     └────────────┘     │
 *   │                                                             │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │   Storage: FxHashMap<String, TinyUrlEntry>                  │
 *   │   TTL: 24 hours (configurable)                             │
 *   │   Cleanup: Background task removes expired entries         │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use rustc_hash::FxHashMap;
use axum::response::IntoResponse;

const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const DEFAULT_TTL_SECS: u64 = 86400; // 24 hours
const MAX_URL_LENGTH: usize = 4096;

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TinyUrlEntry {
    pub id: String,
    pub original_url: String,
    pub short_url: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub hits: u64,
    #[serde(skip)]
    pub last_accessed: Option<Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TinyUrlConfig {
    pub enabled: bool,
    pub base_url: String,
    pub ttl_secs: u64,
    pub max_entries: usize,
    /// HMAC secret used when signing short-URL identifiers.
    ///
    /// **Security:** the [`Default`] value is a well-known placeholder and is
    /// only intended for local development and tests. Override it via
    /// configuration (e.g. `tinyurl.secret` in `portail.toml`) before
    /// exposing the service to untrusted networks. A warning is logged at
    /// startup if the default is detected.
    pub secret: String,
}

/// Well-known placeholder secret used by [`TinyUrlConfig::default`].
/// Override in production — see the field docs on [`TinyUrlConfig::secret`].
pub const DEFAULT_INSECURE_SECRET: &str = "portail-tinyurl-secret";

impl Default for TinyUrlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: "http://localhost:8787".into(),
            ttl_secs: DEFAULT_TTL_SECS,
            max_entries: 100_000,
            secret: DEFAULT_INSECURE_SECRET.into(),
        }
    }
}

// ── Store ────────────────────────────────────────────────────────

pub struct TinyUrlStore {
    entries: std::sync::RwLock<FxHashMap<String, TinyUrlEntry>>,
    config: TinyUrlConfig,
}

impl TinyUrlStore {
    pub fn new(config: TinyUrlConfig) -> Self {
        if config.secret == DEFAULT_INSECURE_SECRET {
            tracing::warn!(
                target: "portail::tinyurl",
                "TinyUrl is using the default placeholder secret; \
                 override `tinyurl.secret` in your config before production use"
            );
        }
        Self {
            entries: std::sync::RwLock::new(FxHashMap::default()),
            config,
        }
    }

    pub fn shorten(&self, url: &str) -> Result<TinyUrlEntry, String> {
        if url.len() > MAX_URL_LENGTH {
            return Err(format!("URL too long (max {} chars)", MAX_URL_LENGTH));
        }

        let id = self.generate_id(url);
        let short_url = format!("{}/s/{}", self.config.base_url, id);
        let now = now_secs();
        
        let entry = TinyUrlEntry {
            id: id.clone(),
            original_url: url.to_string(),
            short_url: short_url.clone(),
            created_at: now,
            expires_at: now + self.config.ttl_secs,
            hits: 0,
            last_accessed: None,
        };

        let mut entries = self.entries.write().unwrap();
        
        // Evict oldest if at capacity
        if entries.len() >= self.config.max_entries {
            if let Some(oldest_key) = entries.iter()
                .min_by_key(|(_, e)| e.created_at)
                .map(|(k, _)| k.clone()) 
            {
                entries.remove(&oldest_key);
            }
        }
        
        entries.insert(id.clone(), entry.clone());
        Ok(entry)
    }

    pub fn resolve(&self, id: &str) -> Option<String> {
        let mut entries = self.entries.write().unwrap();
        let now = now_secs();
        
        if let Some(entry) = entries.get_mut(id) {
            if entry.expires_at > now {
                entry.hits += 1;
                entry.last_accessed = Some(Instant::now());
                return Some(entry.original_url.clone());
            } else {
                entries.remove(id);
            }
        }
        None
    }

    pub fn get_stats(&self) -> TinyUrlStats {
        let entries = self.entries.read().unwrap();
        let now = now_secs();
        
        let total = entries.len();
        let active = entries.values().filter(|e| e.expires_at > now).count();
        let expired = total - active;
        let total_hits = entries.values().map(|e| e.hits).sum();
        
        TinyUrlStats {
            total_entries: total,
            active_entries: active,
            expired_entries: expired,
            total_hits,
        }
    }

    pub fn cleanup_expired(&self) -> usize {
        let mut entries = self.entries.write().unwrap();
        let now = now_secs();
        let before = entries.len();
        entries.retain(|_, e| e.expires_at > now);
        before - entries.len()
    }

    fn generate_id(&self, url: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        self.config.secret.hash(&mut hasher);
        let hash = hasher.finish();
        
        base62_encode(hash)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TinyUrlStats {
    pub total_entries: usize,
    pub active_entries: usize,
    pub expired_entries: usize,
    pub total_hits: u64,
}

// ── Base62 Encoding ──────────────────────────────────────────────

fn base62_encode(mut num: u64) -> String {
    if num == 0 {
        return "0".to_string();
    }
    
    let mut result = Vec::new();
    while num > 0 {
        result.push(BASE62[(num % 62) as usize]);
        num /= 62;
    }
    result.reverse();
    String::from_utf8(result).unwrap()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_shorten(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl axum::response::IntoResponse {
    let url = match req["url"].as_str() {
        Some(u) => u,
        None => return (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"error": "missing url"}))),
    };
    
    match state.tinyurl.shorten(url) {
        Ok(entry) => (axum::http::StatusCode::CREATED, axum::Json(serde_json::to_value(entry).unwrap())),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"error": e}))),
    }
}

pub async fn handle_resolve(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    match state.tinyurl.resolve(&id) {
        Some(url) => {
            axum::response::Redirect::permanent(&url).into_response()
        }
        None => {
            (axum::http::StatusCode::NOT_FOUND, "Short URL not found or expired").into_response()
        }
    }
}

pub async fn handle_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<TinyUrlStats> {
    axum::Json(state.tinyurl.get_stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/tinyurl/shorten", axum::routing::post(handle_shorten))
        .route("/tinyurl/stats", axum::routing::get(handle_stats))
        .route("/s/{id}", axum::routing::get(handle_resolve))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> TinyUrlConfig {
        TinyUrlConfig {
            enabled: true,
            base_url: "http://localhost:8787".into(),
            ttl_secs: 3600,
            max_entries: 1000,
            secret: "test-secret".into(),
        }
    }

    #[test]
    fn shorten_and_resolve() {
        let store = TinyUrlStore::new(test_config());
        let entry = store.shorten("https://example.com/very/long/url").unwrap();
        
        assert!(!entry.id.is_empty());
        assert!(entry.short_url.contains(&entry.id));
        
        let resolved = store.resolve(&entry.id).unwrap();
        assert_eq!(resolved, "https://example.com/very/long/url");
    }

    #[test]
    fn resolve_nonexistent() {
        let store = TinyUrlStore::new(test_config());
        assert!(store.resolve("nonexistent").is_none());
    }

    #[test]
    fn resolve_expired() {
        let config = TinyUrlConfig {
            ttl_secs: 0, // Expire immediately
            ..test_config()
        };
        let store = TinyUrlStore::new(config);
        let entry = store.shorten("https://example.com").unwrap();
        
        // Should be expired
        assert!(store.resolve(&entry.id).is_none());
    }

    #[test]
    fn stats_tracking() {
        let store = TinyUrlStore::new(test_config());
        store.shorten("https://example.com/1").unwrap();
        store.shorten("https://example.com/2").unwrap();
        
        let stats = store.get_stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.active_entries, 2);
    }

    #[test]
    fn base62_encoding() {
        assert_eq!(base62_encode(0), "0");
        assert_eq!(base62_encode(1), "1");
        assert_eq!(base62_encode(61), "z");
        assert_eq!(base62_encode(62), "10");
    }

    #[test]
    fn hit_counter() {
        let store = TinyUrlStore::new(test_config());
        let entry = store.shorten("https://example.com").unwrap();
        
        store.resolve(&entry.id);
        store.resolve(&entry.id);
        
        let stats = store.get_stats();
        assert_eq!(stats.total_hits, 2);
    }
}
