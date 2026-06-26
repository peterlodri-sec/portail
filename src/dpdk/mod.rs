/*
 * DPDK Kernel Bypass Module
 *
 * Architecture:
 *
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    DPDK Kernel Bypass                        │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Traditional Path (kernel):                                │
 *   │   ┌───────────┐     ┌───────────┐     ┌───────────┐        │
 *   │   │ NIC       │────▶│ Kernel    │────▶│ User      │        │
 *   │   │           │     │ Network   │     │ Space     │        │
 *   │   └───────────┘     │ Stack     │     │ (Portail) │        │
 *   │                     └───────────┘     └───────────┘        │
 *   │                          │                                  │
 *   │                     High latency                            │
 *   │                     (context switches)                      │
 *   │                                                             │
 *   │   DPDK Path (kernel bypass):                                │
 *   │   ┌───────────┐     ┌───────────────────────────────────┐   │
 *   │   │ NIC       │────▶│ User Space (Portail + DPDK)       │   │
 *   │   │ (polling) │     │                                   │   │
 *   │   └───────────┘     └───────────────────────────────────┘   │
 *   │                                                             │
 *   │   Low latency                                               │
 *   │   (no kernel involvement)                                   │
 *   │                                                             │
 *   │   DPDK Components:                                          │
 *   │   - PMD (Poll Mode Driver) - Direct NIC access              │
 *   │   - Hugepages - Pre-allocated large memory pages            │
 *   │   - Ring buffers - Lock-free queue between cores            │
 *   │   - Memory pools - Pre-allocated packet buffers             │
 *   │                                                             │
 *   │   Requirements:                                             │
 *   │   - Dedicated NIC (not shared with kernel)                  │
 *   │   - Hugepages configured (2MB or 1GB)                       │
 *   │   - DPDK library installed                                  │
 *   │   - Root or CAP_NET_RAW capability                          │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DpdkConfig {
    pub enabled: bool,
    pub pci_address: String,
    pub hugepage_size: HugepageSize,
    pub hugepage_count: u32,
    pub rx_queues: u32,
    pub tx_queues: u32,
    pub burst_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HugepageSize {
    Size2MB,
    Size1GB,
}

impl Default for DpdkConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Requires dedicated NIC + hugepages
            pci_address: "0000:00:00.0".into(),
            hugepage_size: HugepageSize::Size2MB,
            hugepage_count: 1024,
            rx_queues: 1,
            tx_queues: 1,
            burst_size: 32,
        }
    }
}

// ── DPDK Stats ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DpdkStats {
    pub packets_received: u64,
    pub packets_sent: u64,
    pub packets_dropped: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub rx_pps: u64, // Packets per second
    pub tx_pps: u64,
    pub avg_latency_ns: u64,
    pub queue_depth: u32,
}

// ── DPDK Manager ─────────────────────────────────────────────────

pub struct DpdkManager {
    _config: DpdkConfig,
    stats: std::sync::RwLock<DpdkStats>,
    initialized: bool,
}

impl DpdkManager {
    pub fn new(config: DpdkConfig) -> Self {
        Self {
            _config: config,
            stats: std::sync::RwLock::new(DpdkStats {
                packets_received: 0,
                packets_sent: 0,
                packets_dropped: 0,
                bytes_received: 0,
                bytes_sent: 0,
                rx_pps: 0,
                tx_pps: 0,
                avg_latency_ns: 0,
                queue_depth: 0,
            }),
            initialized: false,
        }
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            // Check if hugepages are configured
            let hugepages = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();

            if !hugepages.contains("HugePages_Free") {
                return Err("Hugepages not configured".into());
            }

            // In a real implementation:
            // 1. Initialize EAL (Environment Abstraction Layer)
            // 2. Configure ports
            // 3. Setup RX/TX queues
            // 4. Start ports

            self.initialized = true;
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err("DPDK is only supported on Linux".into())
        }
    }

    pub fn record_rx(&self, packets: u64, bytes: u64) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.packets_received += packets;
        stats.bytes_received += bytes;
    }

    pub fn record_tx(&self, packets: u64, bytes: u64) {
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.packets_sent += packets;
        stats.bytes_sent += bytes;
    }

    pub fn get_stats(&self) -> DpdkStats {
        self.stats.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

// ── DPDK Packet (simplified) ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DpdkPacket {
    pub data: Vec<u8>,
    pub port: u16,
    pub queue: u16,
    pub timestamp: u64,
}

impl DpdkPacket {
    pub fn new(data: Vec<u8>, port: u16, queue: u16) -> Self {
        Self {
            data,
            port,
            queue,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_dpdk_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<DpdkStats> {
    axum::Json(state.dpdk.get_stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new().route("/dpdk/stats", axum::routing::get(handle_dpdk_stats))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpdk_config_default() {
        let config = DpdkConfig::default();
        assert!(!config.enabled); // Requires dedicated NIC
        assert_eq!(config.burst_size, 32);
    }

    #[test]
    fn dpdk_manager_stats() {
        let mut manager = DpdkManager::new(DpdkConfig::default());
        manager.initialized = true;

        manager.record_rx(10, 1500);
        manager.record_tx(5, 750);

        let stats = manager.get_stats();
        assert_eq!(stats.packets_received, 10);
        assert_eq!(stats.packets_sent, 5);
        assert_eq!(stats.bytes_received, 1500);
    }

    #[test]
    fn dpdk_packet() {
        let pkt = DpdkPacket::new(vec![0; 64], 0, 0);
        assert_eq!(pkt.len(), 64);
        assert!(!pkt.is_empty());
    }
}
