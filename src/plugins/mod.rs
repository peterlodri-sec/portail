//! Portail Plugins
//!
//! Built-in plugins for portail:
//!
//! - tinyurl: URL shortening
//! - tracer: Request/response E2E tracing
//! - redis_cache: App-level Redis cache
//!
//! Routes:
//! - /tinyurl/* - URL shortening
//! - /traces/*  - Request tracing
//! - /cache/*   - Redis cache

pub mod redis_cache;
pub mod tinyurl;
pub mod tracer;

pub use redis_cache::{RedisCache, RedisCacheConfig, CacheStats as RedisStats};
pub use tinyurl::{TinyUrlStore, TinyUrlConfig, TinyUrlEntry};
pub use tracer::{TraceStore, Trace, TraceBuilder};
