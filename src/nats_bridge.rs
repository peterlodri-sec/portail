//! NATS-backed event bus — distributed publish/subscribe.
//!
//! # v1.3
//!
//! Extends the in-memory [`EventLog`] with optional NATS messaging.
//! When connected, events are published to both the local ring buffer
//! and NATS subjects. Remote subscribers can consume events without
//! polling the HTTP API.
//!
//! ## Subject hierarchy
//!
//! ```text
//! portail.events.published   — all events
//! portail.events.{agent_id}  — per-agent events
//! portail.events.{severity}  — severity-filtered (info, warn, error)
//! ```
//!
//! ## Configuration
//!
//! ```toml
//! [nats]
//! url = "nats://localhost:4222"
//! enabled = true
//! ```
//!
//! If NATS is disabled or unavailable, the system degrades gracefully
//! to in-memory-only mode.

use crate::config::Config;
use crate::events::{AgentEvent, EventLog};
use std::sync::Arc;

/// NATS event bridge — receives events from the local ring buffer
/// and publishes them to NATS subjects.
pub struct NatsEventBridge {
    client: Option<async_nats::Client>,
}

impl NatsEventBridge {
    /// Create a new bridge. If NATS is disabled in config, returns
    /// a no-op bridge that silently discards events.
    pub async fn new(config: &Config) -> Self {
        let client = if config.nats_enabled() {
            match async_nats::connect(&config.nats_url()).await {
                Ok(nc) => {
                    tracing::info!(url=%config.nats_url(), "NATS event bridge connected");
                    Some(nc)
                }
                Err(e) => {
                    tracing::warn!(error=%e, "NATS unavailable, event bridge disabled");
                    None
                }
            }
        } else {
            None
        };
        Self { client }
    }

    /// Publish an event to NATS subjects.
    pub async fn publish(&self, event: &AgentEvent) {
        if let Some(ref nc) = self.client {
            let payload = serde_json::to_vec(event).unwrap_or_default();

            // Broad subject — all events
            let _ = nc
                .publish("portail.events.published", payload.clone().into())
                .await;

            // Per-agent subject
            let agent_subject = format!("portail.events.{}", event.agent_id);
            let _ = nc.publish(agent_subject, payload.clone().into()).await;

            // Severity-filtered subject
            let sev_subject = format!("portail.events.{}", event.severity);
            let _ = nc.publish(sev_subject, payload.into()).await;
        }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }
}

/// Spawn a background task that bridges the local EventLog to NATS.
///
/// Reads events from the ring buffer at a regular interval and
/// publishes new ones to NATS subjects.
pub fn spawn_bridge(
    bridge: Arc<NatsEventBridge>,
    event_log: Arc<EventLog>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_index: usize = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let events = event_log.all_since(last_index);
            for event in &events {
                bridge.publish(event).await;
            }
            last_index = last_index.saturating_add(events.len());
        }
    })
}

// ── Config helpers for NATS ───────────────────────────────────────

impl Config {
    pub fn nats_url(&self) -> String {
        std::env::var("PORTAIL_NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into())
    }

    pub fn nats_enabled(&self) -> bool {
        std::env::var("PORTAIL_NATS_ENABLED")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_nats_disabled_by_default() {
        let cfg = Config::default();
        assert!(!cfg.nats_enabled());
        assert_eq!(cfg.nats_url(), "nats://localhost:4222");
    }
}
