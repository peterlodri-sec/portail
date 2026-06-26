use crate::config::CdnConfig;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use super::backend::CacheBackend;
use super::backends::moka::MokaBackend;

/// Cache facade — delegates to a pluggable CacheBackend.
///
/// Defaults to MokaBackend (in-memory + disk tier). Swap to
/// RedisBackend for distributed caching, or rustfs for
/// object-storage backed cache.
pub struct CacheManager {
    backend: Arc<dyn CacheBackend>,
}

impl CacheManager {
    pub fn new(cfg: &CdnConfig) -> Arc<Self> {
        info!(cache_dir = %cfg.cache_dir, cache_size = %cfg.cache_size, "CDN cache init");
        let backend = MokaBackend::new(&cfg.cache_dir, &cfg.cache_size);
        Arc::new(Self { backend })
    }

    /// Create a CacheManager with a custom backend (for testing or alternate impls).
    pub fn with_backend(backend: Arc<dyn CacheBackend>) -> Arc<Self> {
        Arc::new(Self { backend })
    }

    pub async fn get(&self, key: &str) -> Option<Bytes> { self.backend.get(key).await }
    pub async fn put(&self, key: &str, body: Bytes) { self.backend.put(key, body).await }
    pub async fn purge(&self, key: &str) { self.backend.purge(key).await }
    pub async fn purge_prefix(&self, prefix: &str) { self.backend.purge_prefix(prefix).await }
    pub fn stats(&self) -> HashMap<&'static str, u64> { self.backend.stats() }
}

pub async fn stats_logger(cache: Arc<CacheManager>) {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        let s = cache.stats();
        let hits = s.get("hits").copied().unwrap_or(0);
        let misses = s.get("misses").copied().unwrap_or(0);
        let total = hits + misses;
        let ratio = if total > 0 { hits as f64 / total as f64 * 100.0 } else { 0.0 };
        info!(hits, misses, hit_ratio = format_args!("{:.1}%", ratio), "CDN cache stats");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdn::backends::moka::MokaBackend;

    #[test]
    fn parse_size_unit() {
        use crate::cdn::backends::moka::parse_size;
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
        let cache = CacheManager::with_backend(MokaBackend::new("/tmp/_cdn_test_cache", "50m"));
        assert!(cache.get("key1").await.is_none());
        cache.put("key1", Bytes::from("hello")).await;
        assert_eq!(cache.get("key1").await.unwrap(), Bytes::from("hello"));
        cache.purge("key1").await;
        assert!(cache.get("key1").await.is_none());
        let s = cache.stats();
        assert!(*s.get("purges").unwrap() > 0);
    }

    #[tokio::test]
    async fn moka_backend_works() {
        let backend = MokaBackend::new("/tmp/_cdn_test_moka", "10m");
        backend.put("test", Bytes::from("data")).await;
        let result = backend.get("test").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), Bytes::from("data"));
    }
}
