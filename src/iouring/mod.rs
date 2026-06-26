/*
 * io_uring I/O Module
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    io_uring I/O Engine                      │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   User Space                                                │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │                                                     │   │
 *   │   │   ┌───────────┐  ┌───────────┐  ┌───────────┐       │   │
 *   │   │   │ Read      │  │ Write     │  │ Connect   │       │   │
 *   │   │   │ Request   │  │ Request   │  │ Request   │       │   │
 *   │   │   └───────────┘  └───────────┘  └───────────┘       │   │
 *   │   │         │              │              │              │   │
 *   │   │         └──────────────┼──────────────┘              │   │
 *   │   │                        │                             │   │
 *   │   │                        ▼                             │   │
 *   │   │              ┌───────────────────┐                   │   │
 *   │   │              │  Submission Queue │                   │   │
 *   │   │              │  (SQ)             │                   │   │
 *   │   │              └───────────────────┘                   │   │
 *   │   │                        │                             │   │
 *   │   └────────────────────────┼─────────────────────────────┘   │
 *   │                            │                                 │
 *   │   Kernel Space             │                                 │
 *   │   ┌────────────────────────┼─────────────────────────────┐   │
 *   │   │                        ▼                             │   │
 *   │   │              ┌───────────────────┐                   │   │
 *   │   │              │  io_uring         │                   │   │
 *   │   │              │  (kernel)         │                   │   │
 *   │   │              └───────────────────┘                   │   │
 *   │   │                        │                             │   │
 *   │   │                        ▼                             │   │
 *   │   │              ┌───────────────────┐                   │   │
 *   │   │              │  Completion Queue │                   │   │
 *   │   │              │  (CQ)             │                   │   │
 *   │   │              └───────────────────┘                   │   │
 *   │   └────────────────────────┼─────────────────────────────┘   │
 *   │                            │                                 │
 *   │                            ▼                                 │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │              Results / Callbacks                    │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                                                             │
 *   │   Benefits over epoll:                                      │
 *   │   - Single syscall for batch submission                     │
 *   │   - Kernel handles I/O without context switches             │
 *   │   - Zero-copy reads possible                                │
 *   │   - Fixed buffer registration for reduced allocations       │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IoUringConfig {
    pub enabled: bool,
    pub ring_size: u32,
    pub sq_poll: bool,
    pub sq_thread_idle_ms: u32,
    pub fixed_buffers: bool,
    pub registered_files: bool,
}

impl Default for IoUringConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Requires Linux 5.1+
            ring_size: 4096,
            sq_poll: true, // Kernel-side polling for lower latency
            sq_thread_idle_ms: 1000,
            fixed_buffers: true, // Pre-registered buffers
            registered_files: true, // Pre-registered file descriptors
        }
    }
}

// ── io_uring Operation Types ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IoUringOp {
    Read {
        fd: i32,
        buf: Vec<u8>,
        offset: u64,
    },
    Write {
        fd: i32,
        buf: Vec<u8>,
        offset: u64,
    },
    Accept {
        fd: i32,
    },
    Connect {
        fd: i32,
        addr: String,
        port: u16,
    },
    Close {
        fd: i32,
    },
    Timeout {
        duration_ms: u64,
    },
}

// ── io_uring Completion ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IoUringResult {
    pub op: IoUringOp,
    pub result: i32,
    pub latency_ns: u64,
}

// ── io_uring Stats ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IoUringStats {
    pub operations_submitted: u64,
    pub operations_completed: u64,
    pub avg_latency_ns: u64,
    pub queue_depth: u32,
    pub sq_poll_active: bool,
}

// ── io_uring Manager ─────────────────────────────────────────────

pub struct IoUringManager {
    config: IoUringConfig,
    stats: std::sync::RwLock<IoUringStats>,
}

impl IoUringManager {
    pub fn new(config: IoUringConfig) -> Self {
        Self {
            config,
            stats: std::sync::RwLock::new(IoUringStats {
                operations_submitted: 0,
                operations_completed: 0,
                avg_latency_ns: 0,
                queue_depth: 0,
                sq_poll_active: false,
            }),
        }
    }

    pub fn record_submission(&self) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.operations_submitted += 1;
        stats.queue_depth += 1;
    }

    pub fn record_completion(&self, latency_ns: u64) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.operations_completed += 1;
        stats.queue_depth = stats.queue_depth.saturating_sub(1);
        
        // Running average
        let n = stats.operations_completed;
        stats.avg_latency_ns = (stats.avg_latency_ns * (n - 1) + latency_ns) / n;
    }

    pub fn get_stats(&self) -> IoUringStats {
        self.stats.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

// ── io_uring Builder (stub) ──────────────────────────────────────

pub struct IoUringBuilder {
    config: IoUringConfig,
}

impl IoUringBuilder {
    pub fn new() -> Self {
        Self {
            config: IoUringConfig::default(),
        }
    }

    pub fn ring_size(mut self, size: u32) -> Self {
        self.config.ring_size = size;
        self
    }

    pub fn sq_poll(mut self, enabled: bool) -> Self {
        self.config.sq_poll = enabled;
        self
    }

    pub fn build(self) -> Result<IoUringManager, String> {
        #[cfg(target_os = "linux")]
        {
            // Check kernel version
            let version = std::fs::read_to_string("/proc/version")
                .unwrap_or_default();
            
            // Parse kernel version (simplified)
            if !version.contains("Linux version 5.") && !version.contains("Linux version 6.") {
                return Err("io_uring requires Linux 5.1+".into());
            }
            
            Ok(IoUringManager::new(self.config))
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            Err("io_uring is only supported on Linux".into())
        }
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_iouring_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<IoUringStats> {
    axum::Json(state.iouring.get_stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/iouring/stats", axum::routing::get(handle_iouring_stats))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iouring_config_default() {
        let config = IoUringConfig::default();
        assert!(!config.enabled); // Requires Linux 5.1+
        assert_eq!(config.ring_size, 4096);
        assert!(config.sq_poll);
    }

    #[test]
    fn iouring_manager_stats() {
        let manager = IoUringManager::new(IoUringConfig::default());
        
        manager.record_submission();
        manager.record_completion(1000);

        let stats = manager.get_stats();
        assert_eq!(stats.operations_submitted, 1);
        assert_eq!(stats.operations_completed, 1);
        assert_eq!(stats.avg_latency_ns, 1000);
    }
}
