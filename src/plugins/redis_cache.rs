/*
 * Redis Cache Module — App-Level Network-Wide Cache
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    Redis Cache Flow                         │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   App Request                                                │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  GET key   │────▶│  Redis     │────▶│  Miss:     │     │
 *   │   │            │     │  (2GB max) │     │  Fetch +   │     │
 *   │   └────────────┘     └────────────┘     │  SET key   │     │
 *   │        │ hit              │              └────────────┘     │
 *   │        └──────────────────┘                   │             │
 *   │                │                              │             │
 *   │                ▼                              ▼             │
 *   │          Return cached                   Return fresh       │
 *   │                                                             │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │   Use Cases (app-level only, NOT internal):                │
 *   │                                                             │
 *   │   - LLM response caching (same prompt → same response)     │
 *   │   - Agent card caching (A2A discovery)                     │
 *   │   - Hook result caching (expensive hook evaluations)       │
 *   │   - DNS resolution caching (DoH responses)                 │
 *   │   - TinyURL resolution caching                             │
 *   │                                                             │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │   NOT used for:                                             │
 *   │   - Internal metrics                                        │
 *   │   - Health check state                                      │
 *   │   - Event log storage                                       │
 *   │   - Config storage                                          │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use redis::Commands;

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisCacheConfig {
    pub enabled: bool,
    pub url: String,
    pub max_memory_mb: usize,
    pub default_ttl_secs: u64,
    pub key_prefix: String,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            url: "redis://127.0.0.1:6379".into(),
            max_memory_mb: 2048, // 2GB
            default_ttl_secs: 3600,
            key_prefix: "portail:app:".into(),
        }
    }
}

// ── Cache Entry ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T: Serialize> {
    pub key: String,
    pub value: T,
    pub created_at: u64,
    pub ttl_secs: u64,
    pub hits: u64,
}

// ── Redis Cache Store ────────────────────────────────────────────

pub struct RedisCache {
    config: RedisCacheConfig,
    // In-memory fallback when Redis is unavailable
    fallback: std::sync::RwLock<rustc_hash::FxHashMap<String, (String, u64)>>,
}

impl RedisCache {
    pub fn new(config: RedisCacheConfig) -> Self {
        Self {
            config,
            fallback: std::sync::RwLock::new(rustc_hash::FxHashMap::default()),
        }
    }

    pub fn full_key(&self, key: &str) -> String {
        format!("{}{}", self.config.key_prefix, key)
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        let full_key = self.full_key(key);
        
        // Try Redis first
        if let Ok(client) = redis::Client::open(self.config.url.as_str()) {
            if let Ok(mut conn) = client.get_connection() {
                if let Ok(value) = conn.get::<_, String>(&full_key) {
                    return Some(value);
                }
            }
        }
        
        // Fallback to in-memory
        let fallback = self.fallback.read().unwrap();
        if let Some((value, expires_at)) = fallback.get(&full_key) {
            if now_secs() < *expires_at {
                return Some(value.clone());
            }
        }
        None
    }

    pub async fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> bool {
        let full_key = self.full_key(key);
        let ttl = ttl_secs.unwrap_or(self.config.default_ttl_secs);
        
        // Try Redis first
        if let Ok(client) = redis::Client::open(self.config.url.as_str()) {
            if let Ok(mut conn) = client.get_connection() {
                let result = conn.set_ex::<_, _, ()>(&full_key, value, ttl);
                if result.is_ok() {
                    return true;
                }
            }
        }
        
        // Fallback to in-memory
        let mut fallback = self.fallback.write().unwrap();
        fallback.insert(full_key, (value.to_string(), now_secs() + ttl));
        true
    }

    pub async fn delete(&self, key: &str) -> bool {
        let full_key = self.full_key(key);
        
        // Try Redis first
        if let Ok(client) = redis::Client::open(self.config.url.as_str()) {
            if let Ok(mut conn) = client.get_connection() {
                let result = conn.del::<_, ()>(&full_key);
                if result.is_ok() {
                    return true;
                }
            }
        }
        
        // Fallback to in-memory
        let mut fallback = self.fallback.write().unwrap();
        fallback.remove(&full_key);
        true
    }

    pub async fn exists(&self, key: &str) -> bool {
        self.get(key).await.is_some()
    }

    pub async fn stats(&self) -> CacheStats {
        // Try Redis INFO
        if let Ok(client) = redis::Client::open(self.config.url.as_str()) {
            if let Ok(mut conn) = client.get_connection() {
                if let Ok(info) = redis::cmd("INFO").arg("memory").query::<String>(&mut conn) {
                    let used_memory = parse_info_value(&info, "used_memory").unwrap_or(0);
                    let max_memory = parse_info_value(&info, "maxmemory").unwrap_or(
                        self.config.max_memory_mb as u64 * 1024 * 1024
                    );
                    
                    let keys = redis::cmd("DBSIZE").query::<u64>(&mut conn).unwrap_or(0);
                    
                    return CacheStats {
                        connected: true,
                        used_memory_bytes: used_memory,
                        max_memory_bytes: max_memory,
                        keys,
                        hits: parse_info_value(&info, "keyspace_hits").unwrap_or(0),
                        misses: parse_info_value(&info, "keyspace_misses").unwrap_or(0),
                    };
                }
            }
        }
        
        // Fallback stats
        let fallback = self.fallback.read().unwrap();
        CacheStats {
            connected: false,
            used_memory_bytes: 0,
            max_memory_bytes: self.config.max_memory_mb as u64 * 1024 * 1024,
            keys: fallback.len() as u64,
            hits: 0,
            misses: 0,
        }
    }

    pub async fn flush(&self) -> bool {
        // Try Redis FLUSHDB
        if let Ok(client) = redis::Client::open(self.config.url.as_str()) {
            if let Ok(mut conn) = client.get_connection() {
                let result = redis::cmd("FLUSHDB").query::<()>(&mut conn);
                if result.is_ok() {
                    return true;
                }
            }
        }
        
        // Fallback: clear in-memory
        let mut fallback = self.fallback.write().unwrap();
        fallback.clear();
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub connected: bool,
    pub used_memory_bytes: u64,
    pub max_memory_bytes: u64,
    pub keys: u64,
    pub hits: u64,
    pub misses: u64,
}

// ── Helpers ──────────────────────────────────────────────────────

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_info_value(info: &str, key: &str) -> Option<u64> {
    for line in info.lines() {
        if let Some(value) = line.strip_prefix(&format!("{}:", key)) {
            return value.trim().parse().ok();
        }
    }
    None
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_cache_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<CacheStats> {
    axum::Json(state.redis_cache.stats().await)
}

pub async fn handle_cache_get(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.redis_cache.get(&key).await {
        Some(value) => (axum::http::StatusCode::OK, value),
        None => (axum::http::StatusCode::NOT_FOUND, "not found".to_string()),
    }
}

pub async fn handle_cache_set(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl axum::response::IntoResponse {
    let key = match req["key"].as_str() {
        Some(k) => k,
        None => return (axum::http::StatusCode::BAD_REQUEST, "missing key"),
    };
    let value = match req["value"].as_str() {
        Some(v) => v,
        None => return (axum::http::StatusCode::BAD_REQUEST, "missing value"),
    };
    let ttl = req["ttl"].as_u64();
    
    if state.redis_cache.set(key, value, ttl).await {
        (axum::http::StatusCode::OK, "ok")
    } else {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "failed")
    }
}

pub async fn handle_cache_flush(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    if state.redis_cache.flush().await {
        (axum::http::StatusCode::OK, "flushed")
    } else {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "failed")
    }
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/cache/stats", axum::routing::get(handle_cache_stats))
        .route("/cache/{key}", axum::routing::get(handle_cache_get))
        .route("/cache", axum::routing::post(handle_cache_set))
        .route("/cache/flush", axum::routing::post(handle_cache_flush))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let config = RedisCacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_memory_mb, 2048);
        assert_eq!(config.default_ttl_secs, 3600);
        assert_eq!(config.key_prefix, "portail:app:");
    }

    #[test]
    fn full_key_generation() {
        let cache = RedisCache::new(RedisCacheConfig::default());
        assert_eq!(cache.full_key("test"), "portail:app:test");
        assert_eq!(cache.full_key("llm:gpt4"), "portail:app:llm:gpt4");
    }

    #[tokio::test]
    async fn fallback_cache_operations() {
        let cache = RedisCache::new(RedisCacheConfig {
            url: "redis://invalid:6379".into(), // Force fallback
            ..Default::default()
        });
        
        // Set
        cache.set("test_key", "test_value", Some(60)).await;
        
        // Get
        let value = cache.get("test_key").await;
        assert_eq!(value, Some("test_value".to_string()));
        
        // Delete
        cache.delete("test_key").await;
        assert!(cache.get("test_key").await.is_none());
    }

    #[test]
    fn parse_info() {
        let info = "used_memory:12345\nmaxmemory:2048000\n";
        assert_eq!(parse_info_value(info, "used_memory"), Some(12345));
        assert_eq!(parse_info_value(info, "maxmemory"), Some(2048000));
        assert_eq!(parse_info_value(info, "nonexistent"), None);
    }
}
