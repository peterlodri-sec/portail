//! NullClaw — network-native heartbeat agent, powered by ADK-Rust.
//!
//! Replaces the hand-rolled heartbeat loop with an ADK-Rust `CustomAgent`.
//! The agent emits `Heartbeat` events on a configurable interval and can be
//! composed into larger ADK-Rust agent graphs (e.g. fleet monitoring).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Configuration for the heartbeat agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NullClawConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    pub agent_id: String,
}

impl Default for NullClawConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval_secs: 10,
            agent_id: "nullclaw".into(),
        }
    }
}

/// A single heartbeat payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub agent: String,
    pub timestamp: String,
    pub uptime_secs: u64,
    pub requests_processed: u64,
}

/// Shared heartbeat state. Kept independent of ADK-Rust so the main
/// gateway can call `record_request()` cheaply on the hot path.
#[derive(Clone)]
pub struct HeartbeatState {
    agent_id: String,
    started_at: Instant,
    requests_processed: Arc<AtomicU64>,
}

impl HeartbeatState {
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            started_at: Instant::now(),
            requests_processed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn record_request(&self) {
        self.requests_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn generate(&self) -> Heartbeat {
        Heartbeat {
            agent: self.agent_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            requests_processed: self.requests_processed.load(Ordering::Relaxed),
        }
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

/// Build an ADK-Rust `CustomAgent` that emits heartbeats on demand.
///
/// The agent has no LLM dependency; it yields the current heartbeat as an
/// event each time it is invoked. Use `run_nullclaw_loop` for the traditional
/// background heartbeat behavior, or compose the returned agent into an
/// ADK-Rust graph.
pub fn build_heartbeat_agent(
    config: &NullClawConfig,
) -> anyhow::Result<Arc<dyn adk_rust::prelude::Agent>> {
    use adk_rust::InvocationContext;
    use adk_rust::prelude::*;

    let state = HeartbeatState::new(config.agent_id.clone());

    let agent: Arc<dyn Agent> = Arc::new(
        CustomAgentBuilder::new(&config.agent_id)
            .description("Network-native heartbeat agent for Portail")
            .handler(move |_ctx: Arc<dyn InvocationContext>| {
                let state = state.clone();
                async move {
                    let hb = state.generate();
                    tracing::debug!(
                        agent = %hb.agent,
                        uptime = %hb.uptime_secs,
                        requests = %hb.requests_processed,
                        "heartbeat",
                    );

                    let text = serde_json::to_string(&hb).unwrap_or_default();
                    let content = Content::new("model").with_text(text);
                    let mut event = Event::new(&hb.agent);
                    event.author = hb.agent.clone();
                    event.set_content(content);

                    let stream = futures::stream::iter(vec![Ok(event)]);
                    Ok(Box::pin(stream) as EventStream)
                }
            })
            .build()?,
    );

    Ok(agent)
}

/// Run the traditional NullClaw background heartbeat loop.
///
/// Internally this drives an ADK-Rust CustomAgent on a tokio interval.
/// Callers that want deeper integration can use `build_heartbeat_agent`
/// directly and compose it into an ADK-Rust graph.
pub async fn run_nullclaw_loop(config: NullClawConfig) {
    if !config.enabled {
        tracing::info!(agent = %config.agent_id, "NullClaw heartbeat disabled");
        return;
    }

    let agent = match build_heartbeat_agent(&config) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = %e, agent = %config.agent_id, "failed to build NullClaw agent");
            return;
        }
    };

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        config.heartbeat_interval_secs,
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tracing::info!(agent = %config.agent_id, "NullClaw agent started");

    loop {
        interval.tick().await;
        if let Err(e) = invoke_heartbeat(&agent, &config.agent_id).await {
            tracing::warn!(error = %e, agent = %config.agent_id, "heartbeat invocation failed");
        }
    }
}

/// Invoke a single heartbeat step on the agent.
async fn invoke_heartbeat(
    agent: &Arc<dyn adk_rust::prelude::Agent>,
    agent_id: &str,
) -> anyhow::Result<()> {
    use adk_rust::prelude::*;
    use adk_rust::runner::{InvocationContext, MutableSession};
    use adk_session::{CreateRequest, InMemorySessionService, SessionService};
    use futures::StreamExt;

    let app_name = "portail";
    let user_id = "fleet";
    let session_id = uuid::Uuid::new_v4().to_string();
    let invocation_id = format!("{agent_id}-{}-", uuid::Uuid::new_v4());

    let session_service = Arc::new(InMemorySessionService::new());
    let session = session_service
        .create(CreateRequest {
            app_name: app_name.into(),
            user_id: user_id.into(),
            session_id: Some(session_id.clone()),
            state: Default::default(),
        })
        .await?
        .into();
    let mutable_session = Arc::new(MutableSession::new(session));

    let user_content = Content::new("user").with_text("beat");
    let ctx = Arc::new(InvocationContext::with_mutable_session(
        invocation_id,
        Arc::clone(agent) as Arc<dyn Agent>,
        user_id.into(),
        app_name.into(),
        session_id.clone(),
        user_content,
        mutable_session.clone(),
    )?);

    let mut stream = agent.run(ctx).await?;
    while let Some(result) = stream.next().await {
        if let Err(e) = result {
            tracing::debug!(error = %e, "heartbeat event error");
        }
    }

    Ok(())
}
