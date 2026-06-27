//! Moka backend — in-memory cache with TTL and disk tier.
//!
//! Default backend. Uses Moka (lock-free, async, TTL-aware) for
//! hot data and blake3-hashed disk storage for warm data.
//!
//! Performance: <1ms p99 memory hit, <5ms p99 disk hit.

use bytes::Bytes;
use moka::future::Cache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::cdn::backend::CacheBackend;

pub struct MokaBackend {
    memory: Cache<String, Bytes>,
    disk: DiskLayer,
    hits: AtomicU64,
    misses: AtomicU64,
    purges: AtomicU64,
}

struct DiskLayer {
    root: PathBuf,
}

impl DiskLayer {
    fn path(&self, key: &str) -> PathBuf {
        let hash = blake3::hash(key.as_bytes());
        let hex = hash.to_hex();
        self.root
            .join(&hex[..2])
            .join(&hex[2..4])
            .join(hex.as_str())
    }
}

impl MokaBackend {
    pub fn new(cache_dir: &str, cache_size: &str) -> Arc<Self> {
        let max_capacity = parse_size(cache_size).unwrap_or(50_000_000_000);
        let max_entries = (max_capacity / 1_000_000).min(10_000_000);
        let root = PathBuf::from(cache_dir);
        let _ = std::fs::create_dir_all(&root);
        Arc::new(Self {
            memory: Cache::builder()
                .max_capacity(max_entries)
                .time_to_live(Duration::from_secs(3600))
                .build(),
            disk: DiskLayer { root },
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            purges: AtomicU64::new(0),
        })
    }
}

#[async_trait::async_trait]
impl CacheBackend for MokaBackend {
    async fn get(&self, key: &str) -> Option<Bytes> {
        if let Some(body) = self.memory.get(key).await {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Some(body);
        }
        match tokio::fs::read(&self.disk.path(key)).await {
            Ok(data) => {
                let body = Bytes::from(data);
                let _ = self.memory.insert(key.to_string(), body.clone()).await;
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(body)
            }
            Err(_) => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    async fn put(&self, key: &str, body: Bytes) {
        let _ = self.memory.insert(key.to_string(), body.clone()).await;
        let disk_path = self.disk.path(key);
        if let Some(parent) = disk_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&disk_path, &body).await;
    }

    async fn purge(&self, key: &str) {
        self.memory.invalidate(key).await;
        let _ = tokio::fs::remove_file(&self.disk.path(key)).await;
        self.purges.fetch_add(1, Ordering::Relaxed);
    }

    async fn purge_prefix(&self, prefix: &str) {
        let memory_keys: Vec<String> = self
            .memory
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("cdn:{}", prefix)))
            .map(|(k, _)| k.as_ref().clone())
            .collect();
        for key in &memory_keys {
            self.memory.invalidate(key).await;
        }
    }

    fn stats(&self) -> HashMap<&'static str, u64> {
        let mut m = HashMap::new();
        m.insert("hits", self.hits.load(Ordering::Relaxed));
        m.insert("misses", self.misses.load(Ordering::Relaxed));
        m.insert("purges", self.purges.load(Ordering::Relaxed));
        m.insert("memory_entries", self.memory.entry_count());
        m
    }
}

pub(crate) fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    // If the last char is not a digit, it's a unit suffix
    let last_char = s.chars().last()?;
    if last_char.is_ascii_digit() {
        return s.parse().ok();
    }
    let unit = &s[s.len() - 1..];
    let num_str = &s[..s.len() - 1];
    let num = num_str.parse::<u64>().ok()?;
    match unit {
        "k" => Some(num * 1_000),
        "m" => Some(num * 1_000_000),
        "g" => Some(num * 1_000_000_000),
        "t" => Some(num * 1_000_000_000_000),
        _ => Some(num),
    }
}
