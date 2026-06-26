use crate::config::CdnConfig;
use bytes::Bytes;
use moka::future::Cache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tracing::{info, warn};

use futures::StreamExt;

struct DiskLayer {
    root: PathBuf,
}

impl DiskLayer {
    fn path(&self, key: &str) -> PathBuf {
        let hash = blake3::hash(key.as_bytes());
        let hex = hash.to_hex();
        self.root.join(&hex[..2]).join(&hex[2..4]).join(hex.as_str())
    }
}

pub struct CacheManager {
    memory: Cache<String, Bytes>,
    disk: DiskLayer,
    hits: AtomicU64,
    misses: AtomicU64,
    purges: AtomicU64,
}

impl CacheManager {
    pub fn new(cfg: &CdnConfig) -> Arc<Self> {
        let max_capacity = parse_size(&cfg.cache_size).unwrap_or(50_000_000_000);
        let max_entries = (max_capacity / 1_000_000).min(10_000_000);
        info!(cache_dir = %cfg.cache_dir, max_entries, "CDN cache init");
        let root = PathBuf::from(&cfg.cache_dir);
        if let Err(e) = std::fs::create_dir_all(&root) {
            warn!(error = %e, path = %root.display(), "failed to create cache root");
        }
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

    #[inline]
    pub async fn get(&self, key: &str) -> Option<Bytes> {
        if let Some(body) = self.memory.get(key).await {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Some(body);
        }
        let disk_path = self.disk.path(key);
        match fs::read(&disk_path).await {
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

    #[inline]
    pub async fn put(&self, key: &str, body: Bytes) {
        let _ = self.memory.insert(key.to_string(), body.clone()).await;
        let disk_path = self.disk.path(key);
        if let Some(parent) = disk_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                warn!(error = %e, path = %parent.display(), "failed to create subdir");
            }
        }
        if let Err(e) = fs::write(&disk_path, &body).await {
            warn!(error = %e, path = %disk_path.display(), "failed to write disk entry");
        }
    }

    pub async fn purge(&self, key: &str) {
        self.memory.invalidate(key).await;
        let _ = fs::remove_file(&self.disk.path(key)).await;
        self.purges.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn purge_prefix(&self, prefix: &str) {
        let memory_keys: Vec<String> = self
            .memory
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("cdn:{}", prefix)))
            .map(|(k, _)| k.as_ref().clone())
            .collect();
        for key in &memory_keys {
            self.memory.invalidate(key).await;
        }
        let disk_prefix = self.disk.path(&format!("cdn:{}", prefix));
        if let Ok(entries) = tokio::fs::read_dir(&disk_prefix).await {
            use tokio_stream::wrappers::ReadDirStream;
            let mut stream = ReadDirStream::new(entries);
            while let Some(entry) = stream.next().await {
                if let Ok(entry) = entry {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }

    pub fn stats(&self) -> HashMap<&'static str, u64> {
        let mut m = HashMap::new();
        m.insert("hits", self.hits.load(Ordering::Relaxed));
        m.insert("misses", self.misses.load(Ordering::Relaxed));
        m.insert("purges", self.purges.load(Ordering::Relaxed));
        m.insert("memory_entries", self.memory.entry_count());
        m
    }
}

pub async fn stats_logger(cache: Arc<CacheManager>) {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        let hits = cache.hits.load(Ordering::Relaxed);
        let misses = cache.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let ratio = if total > 0 { hits as f64 / total as f64 * 100.0 } else { 0.0 };
        info!(hits, misses, hit_ratio = format_args!("{:.1}%", ratio), "CDN cache stats");
    }
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() { return None; }
    let (num_str, unit) = s.split_at(s.len().max(1) - 1);
    match unit {
        "k" => Some(num_str.parse::<u64>().ok()? * 1_000),
        "m" => Some(num_str.parse::<u64>().ok()? * 1_000_000),
        "g" => Some(num_str.parse::<u64>().ok()? * 1_000_000_000),
        "t" => Some(num_str.parse::<u64>().ok()? * 1_000_000_000_000),
        _ => s.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_unit() {
        assert_eq!(parse_size("50g"), Some(50_000_000_000));
        assert_eq!(parse_size("256m"), Some(256_000_000));
        assert_eq!(parse_size("1t"), Some(1_000_000_000_000));
        assert_eq!(parse_size("1024"), Some(1024));
        assert_eq!(parse_size("0"), Some(0));
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("abc"), None);
    }

    #[tokio::test]
    async fn memory_cache_roundtrip() {
        let cache = Arc::new(CacheManager {
            memory: Cache::builder().max_capacity(100).build(),
            disk: DiskLayer { root: PathBuf::from("/tmp/_cdn_test_cache") },
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            purges: AtomicU64::new(0),
        });
        assert!(cache.get("key1").await.is_none());
        cache.put("key1", Bytes::from("hello")).await;
        assert_eq!(cache.get("key1").await.unwrap(), Bytes::from("hello"));
        cache.purge("key1").await;
        assert!(cache.get("key1").await.is_none());
        let s = cache.stats();
        assert_eq!(*s.get("purges").unwrap(), 1);
    }

    #[tokio::test]
    async fn memory_eviction() {
        let cache = Arc::new(CacheManager {
            memory: Cache::builder().max_capacity(2).build(),
            disk: DiskLayer { root: PathBuf::from("/tmp/_cdn_test_evict") },
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            purges: AtomicU64::new(0),
        });
        cache.put("a", Bytes::from("aaa")).await;
        cache.put("b", Bytes::from("bbb")).await;
        cache.put("c", Bytes::from("ccc")).await;
        let count = cache.memory.entry_count();
        assert!(count <= 2, "entry_count should be capped: got {count}");
    }
}
