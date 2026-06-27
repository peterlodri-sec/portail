//! CI agent runner — schedules and invokes ADK-Rust CI agents.
//!
//! This is the integration point between Portail's server lifecycle and the
//! ADK-Rust agent runtime. It spawns a background task that drives each
//! configured CI agent on a schedule and emits events/traces for results.

use std::sync::Arc;
use std::time::Duration;

/// Schedule for a single CI check.
#[derive(Debug, Clone)]
pub struct CiSchedule {
    pub name: String,
    pub interval_secs: u64,
    pub enabled: bool,
}

/// A runnable CI agent.
pub type CiAgent = Arc<dyn adk_rust::prelude::Agent>;

/// Runner configuration.
#[derive(Debug, Clone)]
pub struct CiRunnerConfig {
    pub schedules: Vec<CiSchedule>,
}

impl Default for CiRunnerConfig {
    fn default() -> Self {
        Self {
            schedules: vec![
                CiSchedule {
                    name: "spec-verify".into(),
                    interval_secs: 300,
                    enabled: true,
                },
                CiSchedule {
                    name: "drift-detect".into(),
                    interval_secs: 900,
                    enabled: false,
                },
                CiSchedule {
                    name: "chore".into(),
                    interval_secs: 3600,
                    enabled: false,
                },
            ],
        }
    }
}

/// Start the CI runner in the background.
///
/// `agents` maps agent name → ADK-Rust agent. The runner spawns one tokio
/// task per schedule and invokes the corresponding agent on each tick.
pub fn spawn_runner(config: CiRunnerConfig, agents: std::collections::HashMap<String, CiAgent>) {
    for schedule in config.schedules {
        if !schedule.enabled {
            continue;
        }
        let Some(agent) = agents.get(&schedule.name).cloned() else {
            tracing::warn!(name = %schedule.name, "no CI agent registered; skipping");
            continue;
        };

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(schedule.interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            tracing::info!(
                name = %schedule.name,
                interval_secs = schedule.interval_secs,
                "CI agent scheduled",
            );

            loop {
                interval.tick().await;
                if let Err(e) = invoke_agent(&agent, &schedule.name).await {
                    tracing::warn!(
                        name = %schedule.name,
                        error = %e,
                        "CI agent invocation failed",
                    );
                }
            }
        });
    }
}

/// Invoke an agent once, logging the result.
async fn invoke_agent(agent: &CiAgent, name: &str) -> anyhow::Result<()> {
    use adk_rust::prelude::*;
    use adk_rust::runner::{InvocationContext, MutableSession};
    use adk_rust::session::{InMemorySessionService, service::CreateRequest};
    use futures::StreamExt;

    let app_name = "portail";
    let user_id = "ci-runner";
    let session_id = uuid::Uuid::new_v4().to_string();
    let invocation_id = format!("{name}-{}-", uuid::Uuid::new_v4());

    let session_service = Arc::new(InMemorySessionService::new());
    let session = session_service
        .create(CreateRequest {
            app_name: app_name.into(),
            user_id: user_id.into(),
            session_id: Some(session_id.clone()),
            state: Default::default(),
        })
        .await?
        .into_dyn();
    let mutable_session = Arc::new(MutableSession::new(session));

    let user_content = Content::new("user").with_text("run");
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
        match result {
            Ok(event) => {
                tracing::debug!(name, "CI agent emitted event");
                if let Some(content) = event.content() {
                    for part in &content.parts {
                        if let Some(text) = part.text() {
                            tracing::debug!(name, payload = %text, "CI agent payload");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!(name, error = %e, "CI agent event error");
            }
        }
    }

    Ok(())
}
