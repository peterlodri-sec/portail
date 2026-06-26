//! NullClaw — network-native heartbeat agent.
//!
//! Runs as a background loop emitting heartbeats with uptime and request stats.
//! Wraps as an ADK-Rust `Agent` via `CustomAgentBuilder`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NullClawConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    pub agent_id: String,
}

impl Default for NullClawConfig {
    fn default() -> Self {
        Self { enabled: true, heartbeat_interval_secs: 10, agent_id: "nullclaw".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub agent: String,
    pub timestamp: String,
    pub uptime_secs: u64,
    pub requests_processed: u64,
}

pub struct NullClaw {
    config: NullClawConfig,
    started_at: Instant,
    requests_processed: AtomicU64,
}

impl NullClaw {
    pub fn new(config: NullClawConfig) -> Self {
        Self { config, started_at: Instant::now(), requests_processed: AtomicU64::new(0) }
    }

    pub fn record_request(&self) {
        self.requests_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn generate_heartbeat(&self) -> Heartbeat {
        Heartbeat {
            agent: self.config.agent_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            requests_processed: self.requests_processed.load(Ordering::Relaxed),
        }
    }

    pub fn interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.heartbeat_interval_secs)
    }

    pub fn id(&self) -> &str {
        &self.config.agent_id
    }
}

pub async fn run_nullclaw_loop(config: NullClawConfig) {
    let agent = Arc::new(NullClaw::new(config.clone()));
    tracing::info!(agent = %config.agent_id, "NullClaw agent started");
    loop {
        tokio::time::sleep(agent.interval()).await;
        let hb = agent.generate_heartbeat();
        tracing::debug!(
            agent = %hb.agent, uptime = %hb.uptime_secs,
            requests = %hb.requests_processed, "heartbeat",
        );
    }
}
