/*
 * Portail Constants
 *
 * Centralized constants, headers, and configuration values used across the codebase.
 * Import this module to ensure consistency and avoid magic numbers.
 *
 * Usage:
 *   use crate::constants::*;
 */

// ── Event System ─────────────────────────────────────────────────

/// Maximum number of events in the ring buffer
pub const MAX_EVENTS: usize = 2000;

/// Broadcast channel capacity for SSE streaming
pub const BROADCAST_CAPACITY: usize = 2048;

// ── Dashboard ────────────────────────────────────────────────────

/// Ring buffer capacity for network samples
pub const DASHBOARD_RING_CAP: usize = 256;

/// Dashboard refresh interval in milliseconds
pub const DASHBOARD_TICK_MS: u64 = 250;

// ── HTTP Headers ─────────────────────────────────────────────────

/// Request ID header - unique identifier for each request
pub const HEADER_REQUEST_ID: &str = "x-request-id";

/// Forwarded For header - client IP chain
pub const HEADER_FORWARDED_FOR: &str = "x-forwarded-for";

/// Portail Proxy header - identifies proxied requests
pub const HEADER_PORTAIL_PROXY: &str = "x-portail-proxy";

/// Cache Status header - HIT or MISS
pub const HEADER_CACHE_STATUS: &str = "x-cache-status";

// ── Hop-by-Hop Headers (stripped during proxy) ───────────────────

/// Headers that should be removed when proxying requests
pub const HOP_BY_HOP_HEADERS: &[&str] = &[
    "host",
    "connection",
    "transfer-encoding",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
];

// ── Proxy Values ─────────────────────────────────────────────────

/// Value for x-portail-proxy header when proxying to AI gateway
pub const PROXY_VALUE_AI_GATEWAY: &str = "ai-gateway";

/// Value for x-portail-proxy header when proxying to A2C
pub const PROXY_VALUE_A2C: &str = "a2c";

/// Value for x-portail-proxy header when proxying to MCP
pub const PROXY_VALUE_MCP: &str = "mcp";

/// Value for x-forwarded-for header
pub const PROXY_FORWARDED_VALUE: &str = "portail";

// ── MCP Protocol ─────────────────────────────────────────────────

/// MCP socket timeout in seconds
pub const MCP_SOCKET_TIMEOUT_SECS: u64 = 30;

/// Maximum body size for MCP requests (10MB)
pub const MCP_MAX_BODY_BYTES: usize = 10_000_000;

// ── TinyURL ──────────────────────────────────────────────────────

/// Default TTL for short URLs (24 hours)
pub const TINYURL_DEFAULT_TTL_SECS: u64 = 86400;

/// Maximum URL length for shortening
pub const TINYURL_MAX_URL_LENGTH: usize = 4096;

/// Maximum number of stored entries
pub const TINYURL_MAX_ENTRIES: usize = 100_000;

/// Default secret for hash generation
pub const TINYURL_DEFAULT_SECRET: &str = "portail-tinyurl-secret";

// ── Redis Cache ──────────────────────────────────────────────────

/// Default Redis URL
pub const REDIS_DEFAULT_URL: &str = "redis://127.0.0.1:6379";

/// Default maximum memory for Redis cache (2GB)
pub const REDIS_MAX_MEMORY_MB: usize = 2048;

/// Default TTL for cache entries (1 hour)
pub const REDIS_DEFAULT_TTL_SECS: u64 = 3600;

/// Key prefix for app-level cache entries
pub const REDIS_KEY_PREFIX: &str = "portail:app:";

// ── Sentinel ─────────────────────────────────────────────────────

/// Sentinel heartbeat interval in seconds
pub const SENTINEL_INTERVAL_SECS: u64 = 30;

// ── NullClaw Agent ───────────────────────────────────────────────

/// NullClaw heartbeat interval in seconds
pub const NULLCLAW_INTERVAL_SECS: u64 = 10;

// ── Godfather Process ────────────────────────────────────────────

/// Godfather heartbeat interval in seconds
pub const GODFATHER_INTERVAL_SECS: u64 = 10;

// ── Network Discovery ────────────────────────────────────────────

/// Discovery heartbeat interval in seconds
pub const DISCOVERY_INTERVAL_SECS: u64 = 30;

/// Node expiry time in seconds (5 minutes)
pub const DISCOVERY_NODE_EXPIRY_SECS: u64 = 300;

/// mDNS service domain
pub const DISCOVERY_MDNS_DOMAIN: &str = "_portail._tcp.local.";

// ── Agent IDs ────────────────────────────────────────────────────

/// Agent ID for sentinel
pub const AGENT_ID_SENTINEL: &str = "sentinel";

/// Agent ID for NullClaw
pub const AGENT_ID_NULLCLAW: &str = "nullclaw";

/// Agent ID for Godfather
pub const AGENT_ID_GODFATHER: &str = "godfather";

/// Agent ID for hooks
pub const AGENT_ID_HOOKS: &str = "hooks";

/// Agent ID for A2C
pub const AGENT_ID_A2C: &str = "a2c";

/// Agent ID for discovery
pub const AGENT_ID_DISCOVERY: &str = "discovery";

// ── Event Types ──────────────────────────────────────────────────

/// Event type for agent started
pub const EVENT_STARTED: &str = "started";

/// Event type for heartbeat
pub const EVENT_HEARTBEAT: &str = "heartbeat";

/// Event type for hook injection
pub const EVENT_HOOK_INJECTED: &str = "injected";

/// Event type for task created (A2A)
pub const EVENT_TASK_CREATED: &str = "task_created";

/// Event type for chat request (A2C)
pub const EVENT_CHAT_REQUEST: &str = "chat_request";

/// Event type for CDN scrub
pub const EVENT_CDN_SCRUB: &str = "cdn_scrub";

/// Event type for health check
pub const EVENT_HEALTH_CHECK: &str = "health_check";

// ── HTTP Status Codes ────────────────────────────────────────────

/// Not Implemented
pub const STATUS_NOT_IMPLEMENTED: u16 = 501;

/// Bad Gateway
pub const STATUS_BAD_GATEWAY: u16 = 502;

/// Service Unavailable
pub const STATUS_SERVICE_UNAVAILABLE: u16 = 503;

// ── Timeouts ─────────────────────────────────────────────────────

/// Gateway request timeout in seconds
pub const GATEWAY_TIMEOUT_SECS: u64 = 600;

/// HTTP/2 keep-alive interval in seconds
pub const HTTP2_KEEPALIVE_SECS: u64 = 30;

// ── File Paths ───────────────────────────────────────────────────

/// Default config file path
pub const DEFAULT_CONFIG_PATH: &str = "portail.toml";

/// Default MCP socket path
pub const DEFAULT_MCP_SOCKET: &str = "/run/portail/mcp.sock";

/// Default cache directory
pub const DEFAULT_CACHE_DIR: &str = "/var/cache/portail";

/// Default cache size
pub const DEFAULT_CACHE_SIZE: &str = "10g";

// ── Network ──────────────────────────────────────────────────────

/// Default listen address
pub const DEFAULT_LISTEN: &str = "0.0.0.0:8787";

/// Default AI gateway upstream
pub const DEFAULT_AI_UPSTREAM: &str = "http://127.0.0.1:4000";

/// Default CDN origin
pub const DEFAULT_CDN_ORIGIN: &str = "http://127.0.0.1:9000";

// ── DoH Endpoints ────────────────────────────────────────────────

/// Cloudflare DoH endpoint
pub const DOH_CLOUDFLARE: &str = "https://cloudflare-dns.com/dns-query";

/// Google DoH endpoint
pub const DOH_GOOGLE: &str = "https://dns.google/dns-query";

// ── Version ──────────────────────────────────────────────────────

/// Portail version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Portail name
pub const NAME: &str = "portail";

/// Portail description
pub const DESCRIPTION: &str = "Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_consistent() {
        // Ensure agent IDs are unique
        let agent_ids = [
            AGENT_ID_SENTINEL,
            AGENT_ID_NULLCLAW,
            AGENT_ID_GODFATHER,
            AGENT_ID_HOOKS,
            AGENT_ID_A2C,
            AGENT_ID_DISCOVERY,
        ];
        let unique: std::collections::HashSet<_> = agent_ids.iter().collect();
        assert_eq!(unique.len(), agent_ids.len());

        // Ensure event types are unique
        let event_types = [
            EVENT_STARTED,
            EVENT_HEARTBEAT,
            EVENT_HOOK_INJECTED,
            EVENT_TASK_CREATED,
            EVENT_CHAT_REQUEST,
            EVENT_CDN_SCRUB,
            EVENT_HEALTH_CHECK,
        ];
        let unique: std::collections::HashSet<_> = event_types.iter().collect();
        assert_eq!(unique.len(), event_types.len());
    }

    #[test]
    fn headers_are_lowercase() {
        assert_eq!(HEADER_REQUEST_ID, HEADER_REQUEST_ID.to_lowercase());
        assert_eq!(HEADER_FORWARDED_FOR, HEADER_FORWARDED_FOR.to_lowercase());
        assert_eq!(HEADER_PORTAIL_PROXY, HEADER_PORTAIL_PROXY.to_lowercase());
        assert_eq!(HEADER_CACHE_STATUS, HEADER_CACHE_STATUS.to_lowercase());
    }
}
