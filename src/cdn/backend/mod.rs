//! Cache backend trait — pluggable caching abstraction.
//!
//! # v2.x — SOTA Abstraction
//!
//! All cache implementations implement this trait. Swap backends
//! without changing any application code.

use bytes::Bytes;
use std::collections::HashMap;

/// Core cache operations. All backends implement this.
///
/// # Performance contract
///
/// - `get()` must return in <5ms p99 for in-memory backends
/// - `put()` is fire-and-forget (no guarantee of durability)
/// - `stats()` is informational, not a consistency guarantee
#[async_trait::async_trait]
pub trait CacheBackend: Send + Sync + 'static {
    async fn get(&self, key: &str) -> Option<Bytes>;
    async fn put(&self, key: &str, data: Bytes);
    async fn purge(&self, key: &str);
    async fn purge_prefix(&self, prefix: &str);
    fn stats(&self) -> HashMap<&'static str, u64>;
}
