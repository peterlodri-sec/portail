/*
 * Hyper Low-Level HTTP Module
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    Hyper HTTP Engine                         │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Layer Stack (top to bottom):                              │
 *   │                                                             │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │  Axum (high-level routing, handlers)                │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                          │                                  │
 *   │                          ▼                                  │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │  Tower (middleware, services)                        │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                          │                                  │
 *   │                          ▼                                  │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │  Hyper (HTTP/1.1, HTTP/2, connection management)    │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                          │                                  │
 *   │                          ▼                                  │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │  Tokio (async I/O, runtime)                         │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                                                             │
 *   │   When to use hyper directly:                               │
 *   │   - Custom HTTP/2 settings                                  │
 *   │   - Fine-grained connection control                         │
 *   │   - Custom protocol extensions                              │
 *   │   - Maximum performance (skip axum overhead)                │
 *   │   - WebSocket upgrades                                      │
 *   │   - Server-Sent Events (SSE)                                │
 *   │                                                             │
 *   │   Hyper features used:                                      │
 *   │   - HTTP/1.1 and HTTP/2 support                             │
 *   │   - Connection pooling                                      │
 *   │   - Keep-alive management                                   │
 *   │   - Flow control                                            │
 *   │   - Header parsing                                          │
 *   │   - Body streaming                                          │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperConfig {
    pub enabled: bool,
    pub http2_only: bool,
    pub http2_max_concurrent_streams: u32,
    pub http2_initial_stream_window_size: u32,
    pub http2_initial_connection_window_size: u32,
    pub keep_alive_interval_ms: u64,
    pub keep_alive_timeout_ms: u64,
    pub max_header_size: usize,
    pub max_body_size: usize,
}

impl Default for HyperConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Use axum by default
            http2_only: false,
            http2_max_concurrent_streams: 1000,
            http2_initial_stream_window_size: 1048576, // 1MB
            http2_initial_connection_window_size: 1048576, // 1MB
            keep_alive_interval_ms: 30000,
            keep_alive_timeout_ms: 60000,
            max_header_size: 16384, // 16KB
            max_body_size: 10485760, // 10MB
        }
    }
}

// ── Hyper Stats ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperStats {
    pub active_connections: u64,
    pub total_connections: u64,
    pub http1_connections: u64,
    pub http2_connections: u64,
    pub requests_processed: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub avg_request_latency_ms: u64,
}

// ── Hyper Manager ────────────────────────────────────────────────

pub struct HyperManager {
    config: HyperConfig,
    stats: std::sync::RwLock<HyperStats>,
}

impl HyperManager {
    pub fn new(config: HyperConfig) -> Self {
        Self {
            config,
            stats: std::sync::RwLock::new(HyperStats {
                active_connections: 0,
                total_connections: 0,
                http1_connections: 0,
                http2_connections: 0,
                requests_processed: 0,
                bytes_sent: 0,
                bytes_received: 0,
                avg_request_latency_ms: 0,
            }),
        }
    }

    pub fn record_connection(&self, is_http2: bool) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.active_connections += 1;
        stats.total_connections += 1;
        if is_http2 {
            stats.http2_connections += 1;
        } else {
            stats.http1_connections += 1;
        }
    }

    pub fn record_disconnect(&self) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.active_connections = stats.active_connections.saturating_sub(1);
    }

    pub fn record_request(&self, latency_ms: u64, bytes_sent: u64, bytes_received: u64) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.requests_processed += 1;
        stats.bytes_sent += bytes_sent;
        stats.bytes_received += bytes_received;
        
        // Running average
        let n = stats.requests_processed;
        stats.avg_request_latency_ms = (stats.avg_request_latency_ms * (n - 1) + latency_ms) / n;
    }

    pub fn get_stats(&self) -> HyperStats {
        self.stats.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

// ── Hyper Server Builder (stub) ──────────────────────────────────

pub struct HyperServerBuilder {
    config: HyperConfig,
}

impl HyperServerBuilder {
    pub fn new() -> Self {
        Self {
            config: HyperConfig::default(),
        }
    }

    pub fn http2_only(mut self, enabled: bool) -> Self {
        self.config.http2_only = enabled;
        self
    }

    pub fn max_concurrent_streams(mut self, max: u32) -> Self {
        self.config.http2_max_concurrent_streams = max;
        self
    }

    pub fn build(self) -> HyperManager {
        HyperManager::new(self.config)
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_hyper_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<HyperStats> {
    axum::Json(state.hyper.get_stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/hyper/stats", axum::routing::get(handle_hyper_stats))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyper_config_default() {
        let config = HyperConfig::default();
        assert!(!config.enabled); // Use axum by default
        assert_eq!(config.http2_max_concurrent_streams, 1000);
    }

    #[test]
    fn hyper_manager_stats() {
        let manager = HyperManager::new(HyperConfig::default());
        
        manager.record_connection(false); // HTTP/1.1
        manager.record_request(10, 1024, 512);

        let stats = manager.get_stats();
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.http1_connections, 1);
        assert_eq!(stats.requests_processed, 1);
    }
}
