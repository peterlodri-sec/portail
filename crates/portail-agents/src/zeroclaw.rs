//! Zeroclaw agent — manages the zeroclaw process as a Portail agent.
//!
//! Zeroclaw (https://github.com/zeroclaw-labs/zeroclaw) is a Rust agent runtime
//! with an integrated HTTP/WebSocket gateway. Portail spawns it as a sidecar
//! process and proxies MCP requests to its gateway API.
//!
//! This agent:
//! 1. Spawns `zeroclaw gateway start` with gateway enabled
//! 2. Monitors its health via periodic heartbeat checks
//! 3. Exposes a `run_zeroclaw_loop()` for background lifecycle management
//! 4. Reports status through the Portail agent ecosystem

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::process::Command;

/// Default zeroclaw gateway host.
pub const DEFAULT_ZEROCLAW_HOST: &str = "127.0.0.1";

/// Default zeroclaw gateway port.
pub const DEFAULT_ZEROCLAW_PORT: u16 = 42617;

/// Default zeroclaw binary name.
pub const DEFAULT_ZEROCLAW_BIN: &str = "zeroclaw";

/// Zeroclaw agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroclawConfig {
    /// Whether the zeroclaw sidecar is enabled.
    pub enabled: bool,
    /// Path or name of the zeroclaw binary.
    pub binary: String,
    /// Gateway host to bind to.
    pub host: String,
    /// Gateway port to listen on (0 = random available).
    pub port: u16,
    /// Interval between heartbeats in seconds.
    pub heartbeat_interval_secs: u64,
    /// Agent identifier.
    pub agent_id: String,
    /// Additional zeroclaw CLI flags.
    pub extra_args: Vec<String>,
}

impl Default for ZeroclawConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            binary: DEFAULT_ZEROCLAW_BIN.to_string(),
            host: DEFAULT_ZEROCLAW_HOST.to_string(),
            port: DEFAULT_ZEROCLAW_PORT,
            heartbeat_interval_secs: 30,
            agent_id: "zeroclaw".into(),
            extra_args: Vec::new(),
        }
    }
}

/// Zeroclaw sidecar process state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ZeroclawState {
    Starting,
    Running,
    Degraded(String),
    Stopped,
}

/// Heartbeat payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroclawHeartbeat {
    pub agent: String,
    pub timestamp: String,
    pub uptime_secs: u64,
    pub state: ZeroclawState,
    pub gateway_url: String,
    pub requests_proxied: u64,
}

/// Zeroclaw sidecar agent.
pub struct ZeroclawAgent {
    config: ZeroclawConfig,
    started_at: Instant,
    state: tokio::sync::RwLock<ZeroclawState>,
    requests_proxied: AtomicU64,
    child_pid: AtomicU64,
}

impl ZeroclawAgent {
    /// Create a new zeroclaw sidecar agent.
    pub fn new(config: ZeroclawConfig) -> Self {
        Self {
            started_at: Instant::now(),
            state: tokio::sync::RwLock::new(ZeroclawState::Starting),
            requests_proxied: AtomicU64::new(0),
            child_pid: AtomicU64::new(0),
            config,
        }
    }

    /// The gateway URL for this zeroclaw instance.
    pub fn gateway_url(&self) -> String {
        format!("http://{}:{}", self.config.host, self.config.port)
    }

    /// Spawn the zeroclaw daemon as a subprocess.
    pub async fn spawn(&self) -> anyhow::Result<tokio::process::Child> {
        let mut cmd = Command::new(&self.config.binary);
        cmd.arg("gateway")
            .arg("start")
            .arg("--host")
            .arg(&self.config.host)
            .arg("--port")
            .arg(self.config.port.to_string())
            .kill_on_drop(true);

        for arg in &self.config.extra_args {
            cmd.arg(arg);
        }

        let child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn zeroclaw: {e}"))?;

        self.child_pid
            .store(u64::from(child.id().unwrap_or(0)), Ordering::Relaxed);
        tracing::info!(
            pid = child.id().unwrap_or(0),
            url = %self.gateway_url(),
            "Zeroclaw daemon spawned"
        );

        Ok(child)
    }

    /// Check if the zeroclaw gateway is reachable.
    pub async fn check_health(&self) -> anyhow::Result<()> {
        let url = format!("{}/health/liveliness", self.gateway_url());
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| anyhow::anyhow!("zeroclaw health check failed: {e}"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("zeroclaw returned {}", resp.status());
        }
    }

    /// Update the agent state.
    pub async fn set_state(&self, state: ZeroclawState) {
        let mut s = self.state.write().await;
        *s = state;
    }

    /// Get the current agent state.
    pub async fn current_state(&self) -> ZeroclawState {
        self.state.read().await.clone()
    }

    /// Record a proxied request.
    pub fn record_request(&self) {
        self.requests_proxied.fetch_add(1, Ordering::Relaxed);
    }

    /// Generate a heartbeat payload.
    pub async fn generate_heartbeat(&self) -> ZeroclawHeartbeat {
        ZeroclawHeartbeat {
            agent: self.config.agent_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            state: self.current_state().await,
            gateway_url: self.gateway_url(),
            requests_proxied: self.requests_proxied.load(Ordering::Relaxed),
        }
    }

    pub fn id(&self) -> &str {
        &self.config.agent_id
    }

    pub fn interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.heartbeat_interval_secs)
    }
}

/// Run the zeroclaw sidecar lifecycle loop.
pub async fn run_zeroclaw_loop(config: ZeroclawConfig) -> anyhow::Result<()> {
    let agent = Arc::new(ZeroclawAgent::new(config.clone()));
    tracing::info!(agent = %config.agent_id, "Zeroclaw agent starting");

    let mut child = agent.spawn().await?;

    agent.set_state(ZeroclawState::Starting).await;
    for i in 0..30 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        match agent.check_health().await {
            Ok(()) => {
                agent.set_state(ZeroclawState::Running).await;
                tracing::info!(
                    url = %agent.gateway_url(),
                    "Zeroclaw gateway is healthy after {}s",
                    i + 1
                );
                break;
            }
            Err(e) => {
                if i == 29 {
                    tracing::warn!(error = %e, "Zeroclaw gateway never became healthy");
                    agent
                        .set_state(ZeroclawState::Degraded(e.to_string()))
                        .await;
                }
            }
        }
    }

    loop {
        tokio::time::sleep(agent.interval()).await;

        match agent.check_health().await {
            Ok(()) => {
                if agent.current_state().await != ZeroclawState::Running {
                    agent.set_state(ZeroclawState::Running).await;
                }
            }
            Err(e) => {
                agent
                    .set_state(ZeroclawState::Degraded(e.to_string()))
                    .await;
                tracing::warn!(error = %e, "Zeroclaw gateway health check failed");
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                agent.set_state(ZeroclawState::Stopped).await;
                tracing::error!(
                    exit_code = status.code().unwrap_or(-1),
                    "Zeroclaw process exited"
                );
                anyhow::bail!("zeroclaw process exited with status {status}");
            }
            Ok(None) => {
                let hb = agent.generate_heartbeat().await;
                tracing::debug!(
                    agent = %hb.agent,
                    uptime = %hb.uptime_secs,
                    state = ?hb.state,
                    requests = %hb.requests_proxied,
                    "zeroclaw heartbeat",
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to check zeroclaw process");
            }
        }
    }
}
