/*
 * eBPF Observability Module
 * 
 * Architecture:
 * 
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    eBPF Observability                       │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Kernel Space (eBPF programs)                              │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │  ┌───────────┐  ┌───────────┐  ┌───────────┐       │   │
 *   │   │  │ Syscall   │  │ Network   │  │ Scheduler │       │   │
 *   │   │  │ Tracer    │  │ Latency   │  │ Monitor   │       │   │
 *   │   │  └───────────┘  └───────────┘  └───────────┘       │   │
 *   │   │         │              │              │              │   │
 *   │   │         └──────────────┼──────────────┘              │   │
 *   │   │                        │                             │   │
 *   │   │                        ▼                             │   │
 *   │   │              ┌───────────────────┐                   │   │
 *   │   │              │  BPF Maps         │                   │   │
 *   │   │              │  (ringbuf/perf)   │                   │   │
 *   │   │              └───────────────────┘                   │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                          │                                  │
 *   │                          ▼                                  │
 *   │   User Space (Portail)                                      │   │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │  ┌───────────┐  ┌───────────┐  ┌───────────┐       │   │
 *   │   │  │ Event     │  │ Metrics   │  │ Trace     │       │   │
 *   │   │  │ Reader    │  │ Collector │  │ Aggregator│       │   │
 *   │   │  └───────────┘  └───────────┘  └───────────┘       │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                                                             │
 *   │   eBPF Programs:                                            │
 *   │   1. syscall_trace - Trace syscalls (read/write/connect)    │
 *   │   2. network_latency - Measure TCP RTT                      │
 *   │   3. scheduler_trace - Track task scheduling latency        │
 *   │   4. memory_tracker - Monitor memory allocations            │
 *   │                                                             │
 *   │   BPF Maps:                                                 │
 *   │   - Ring buffer for events                                  │
 *   │   - Per-CPU arrays for counters                             │
 *   │   - Hash maps for state tracking                            │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EbpfConfig {
    pub enabled: bool,
    pub trace_syscalls: bool,
    pub trace_network: bool,
    pub trace_scheduler: bool,
    pub trace_memory: bool,
    pub sample_rate: u32,
}

impl Default for EbpfConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Requires root/CAP_BPF
            trace_syscalls: true,
            trace_network: true,
            trace_scheduler: false,
            trace_memory: false,
            sample_rate: 100, // Sample 1 in 100 events
        }
    }
}

// ── eBPF Event Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EbpfEvent {
    pub timestamp: u64,
    pub event_type: EbpfEventType,
    pub pid: u32,
    pub tid: u32,
    pub comm: String,
    pub data: EbpfEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EbpfEventType {
    Syscall,
    NetworkLatency,
    SchedulerLatency,
    MemoryAllocation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EbpfEventData {
    Syscall {
        syscall_nr: u32,
        syscall_name: String,
        duration_ns: u64,
        return_value: i64,
    },
    NetworkLatency {
        src_ip: String,
        dst_ip: String,
        src_port: u16,
        dst_port: u16,
        rtt_ns: u64,
    },
    SchedulerLatency {
        prev_pid: u32,
        next_pid: u32,
        latency_ns: u64,
    },
    MemoryAllocation {
        size: u64,
        addr: u64,
        is_free: bool,
    },
}

// ── eBPF Stats ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EbpfStats {
    pub events_processed: u64,
    pub syscalls_traced: u64,
    pub network_events: u64,
    pub scheduler_events: u64,
    pub memory_events: u64,
    pub avg_syscall_latency_ns: u64,
    pub avg_network_rtt_ns: u64,
}

// ── eBPF Manager ─────────────────────────────────────────────────

pub struct EbpfManager {
    config: EbpfConfig,
    stats: std::sync::RwLock<EbpfStats>,
}

impl EbpfManager {
    pub fn new(config: EbpfConfig) -> Self {
        Self {
            config,
            stats: std::sync::RwLock::new(EbpfStats {
                events_processed: 0,
                syscalls_traced: 0,
                network_events: 0,
                scheduler_events: 0,
                memory_events: 0,
                avg_syscall_latency_ns: 0,
                avg_network_rtt_ns: 0,
            }),
        }
    }

    pub fn record_event(&self, event: EbpfEvent) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.events_processed += 1;
        
        match &event.data {
            EbpfEventData::Syscall { duration_ns, .. } => {
                stats.syscalls_traced += 1;
                // Running average
                stats.avg_syscall_latency_ns = 
                    (stats.avg_syscall_latency_ns * (stats.syscalls_traced - 1) + duration_ns) 
                    / stats.syscalls_traced;
            }
            EbpfEventData::NetworkLatency { rtt_ns, .. } => {
                stats.network_events += 1;
                stats.avg_network_rtt_ns = 
                    (stats.avg_network_rtt_ns * (stats.network_events - 1) + rtt_ns) 
                    / stats.network_events;
            }
            EbpfEventData::SchedulerLatency { .. } => {
                stats.scheduler_events += 1;
            }
            EbpfEventData::MemoryAllocation { .. } => {
                stats.memory_events += 1;
            }
        }
    }

    pub fn get_stats(&self) -> EbpfStats {
        self.stats.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

// ── eBPF Program Loader (stub) ───────────────────────────────────

pub struct EbpfProgram {
    name: String,
    loaded: bool,
}

impl EbpfProgram {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            loaded: false,
        }
    }

    pub fn load(&mut self) -> Result<(), String> {
        // In a real implementation, this would:
        // 1. Read the eBPF bytecode from a file
        // 2. Load it into the kernel using libbpf or aya
        // 3. Attach to tracepoints/kprobes
        
        #[cfg(target_os = "linux")]
        {
            // Check if eBPF is supported
            if !std::path::Path::new("/sys/fs/bpf").exists() {
                return Err("eBPF filesystem not mounted".into());
            }
            
            // Check if we have CAP_BPF
            // In production, use caps crate
            
            self.loaded = true;
            Ok(())
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            Err("eBPF is only supported on Linux".into())
        }
    }

    pub fn unload(&mut self) {
        self.loaded = false;
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_ebpf_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<EbpfStats> {
    axum::Json(state.ebpf.get_stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/ebpf/stats", axum::routing::get(handle_ebpf_stats))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ebpf_config_default() {
        let config = EbpfConfig::default();
        assert!(!config.enabled); // Requires root
        assert!(config.trace_syscalls);
        assert!(config.trace_network);
    }

    #[test]
    fn ebpf_manager_stats() {
        let manager = EbpfManager::new(EbpfConfig::default());
        
        manager.record_event(EbpfEvent {
            timestamp: 0,
            event_type: EbpfEventType::Syscall,
            pid: 1234,
            tid: 1234,
            comm: "test".into(),
            data: EbpfEventData::Syscall {
                syscall_nr: 1,
                syscall_name: "read".into(),
                duration_ns: 1000,
                return_value: 100,
            },
        });

        let stats = manager.get_stats();
        assert_eq!(stats.events_processed, 1);
        assert_eq!(stats.syscalls_traced, 1);
    }
}
