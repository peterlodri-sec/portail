//! Redis backend — distributed cache via Redis.
//!
//! Optional backend. Requires a running Redis instance.
//! Keys are prefixed with `portail:cdn:` for namespace isolation.
//! TTL is set per-entry (default 1h).

use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cdn::backend::CacheBackend;

pub struct RedisBackend {
    hits: AtomicU64,
    misses: AtomicU64,
    // In production, this would hold a redis::aio::ConnectionManager
    // or a connection pool. Stub for now.
}

impl RedisBackend {
    pub fn new(_redis_url: &str) -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl CacheBackend for RedisBackend {
    async fn get(&self, _key: &str) -> Option<Bytes> {
        // Stub: would use redis::cmd("GET").arg(key).query_async(&mut conn)
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    async fn put(&self, _key: &str, _data: Bytes) {
        // Stub: would use redis::cmd("SET").arg(key).arg(data).arg("EX").arg(3600)
    }

    async fn purge(&self, _key: &str) {
        // Stub: would use redis::cmd("DEL").arg(key)
    }

    async fn purge_prefix(&self, _prefix: &str) {
        // Stub: would use SCAN + DEL pattern
    }

    fn stats(&self) -> HashMap<&'static str, u64> {
        let mut m = HashMap::new();
        m.insert("hits", self.hits.load(Ordering::Relaxed));
        m.insert("misses", self.misses.load(Ordering::Relaxed));
        m
    }
}
