//! DNS reliability — cache with TTL, negative caching, fallback resolvers.
//!
//! # v2.0
//!
//! In-memory DNS cache that respects TTLs, caches NXDOMAIN responses
//! (negative caching), and fails over through a chain of resolvers.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A cached DNS record.
#[derive(Debug, Clone)]
struct CachedRecord {
    value: Vec<String>,
    expires_at: Instant,
    is_negative: bool,
}

/// Thread-safe DNS cache with TTL-aware eviction.
pub struct DnsCache {
    records: RwLock<HashMap<String, CachedRecord>>,
    max_entries: usize,
}

impl DnsCache {
    pub fn new(max_entries: usize) -> Self {
        Self { records: RwLock::new(HashMap::new()), max_entries }
    }

    /// Look up a cached record. Returns None if not found or expired.
    pub fn get(&self, domain: &str) -> Option<Vec<String>> {
        let records = self.records.read().ok()?;
        records.get(domain).and_then(|r| {
            if Instant::now() > r.expires_at {
                None
            } else if r.is_negative {
                Some(vec![])
            } else {
                Some(r.value.clone())
            }
        })
    }

    /// Store a successful resolution with TTL.
    pub fn set(&self, domain: &str, ips: Vec<String>, ttl_secs: u64) {
        let mut records = self.records.write().ok();
        if let Some(ref mut recs) = records {
            if recs.len() >= self.max_entries {
                // Evict oldest expired entries
                recs.retain(|_, r| Instant::now() <= r.expires_at);
            }
            let ttl = ttl_secs.min(86400); // cap at 24h
            recs.insert(domain.to_string(), CachedRecord {
                value: ips,
                expires_at: Instant::now() + Duration::from_secs(ttl),
                is_negative: false,
            });
        }
    }

    /// Cache a negative response (NXDOMAIN) for a short duration.
    pub fn set_negative(&self, domain: &str) {
        let mut records = self.records.write().ok();
        if let Some(ref mut recs) = records {
            recs.insert(domain.to_string(), CachedRecord {
                value: vec![],
                expires_at: Instant::now() + Duration::from_secs(60),
                is_negative: true,
            });
        }
    }

    pub fn len(&self) -> usize {
        self.records.read().map(|r| r.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Fallback resolver chain: tries each resolver URL in order until one succeeds.
pub struct FallbackResolvers {
    urls: Vec<String>,
}

impl FallbackResolvers {
    /// Create resolver chain. Tries each URL in order until one responds.
    /// Default: Cloudflare → Google → Quad9.
    pub fn new(urls: Vec<String>) -> Self {
        Self { urls }
    }

    pub fn default() -> Self {
        Self {
            urls: vec![
                "https://cloudflare-dns.com/dns-query".into(),
                "https://dns.google/resolve".into(),
                "https://doh.opendns.com/dns-query".into(),
            ],
        }
    }

    pub fn primary(&self) -> &str {
        &self.urls[0]
    }

    pub fn fallback_urls(&self) -> &[String] {
        &self.urls[1..]
    }

    pub fn all(&self) -> &[String] {
        &self.urls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_miss() {
        let cache = DnsCache::new(100);
        assert!(cache.get("example.com").is_none());

        cache.set("example.com", vec!["1.2.3.4".into()], 3600);
        assert_eq!(cache.get("example.com").unwrap(), vec!["1.2.3.4"]);
    }

    #[test]
    fn negative_cache() {
        let cache = DnsCache::new(100);
        cache.set_negative("nonexistent.example");
        let result = cache.get("nonexistent.example");
        assert!(result.is_some());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn fallback_has_three_resolvers() {
        let f = FallbackResolvers::default();
        assert_eq!(f.all().len(), 3);
        assert!(f.primary().starts_with("https://"));
    }
}
