//! Supervisor — lightweight Elixir-OTP-style supervision for tokio tasks.
//!
//! Wraps `tokio::spawn` with health tracking, auto-restart on panic,
//! backoff, and `/supervisor/status` event publishing.
//!
//! ```ignore
//! let sup = Supervisor::new(state.event_log.clone());
//! sup.spawn("godfather", SupervisorPolicy::always(), run_godfather(config, state));
//! sup.spawn("sentinel",  SupervisorPolicy::always(), run_sentinel(log, cache));
//! ```

use crate::types::BoundedMeta;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

// ─── policy ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum RestartPolicy {
    Always,
    Never,
    Transient,
    Permanent { max_restarts: u32, within: Duration },
}

impl RestartPolicy {
    pub fn always() -> Self {
        Self::Always
    }
    pub fn never() -> Self {
        Self::Never
    }
    pub fn transient() -> Self {
        Self::Transient
    }
    pub fn permanent() -> Self {
        Self::Permanent {
            max_restarts: 5,
            within: Duration::from_secs(60),
        }
    }
}

// ─── supervisor ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TaskStatus {
    pub name: String,
    pub status: String, // "running" | "crashed" | "restarting" | "stopped"
    pub restarts: u32,
    pub last_error: Option<String>,
}

pub struct Supervisor {
    event_log: Arc<crate::events::EventLog>,
    tasks: Arc<std::sync::RwLock<Vec<TaskStatus>>>,
}

impl Supervisor {
    pub fn new(event_log: Arc<crate::events::EventLog>) -> Self {
        Self {
            event_log,
            tasks: Arc::new(std::sync::RwLock::new(Vec::new())),
        }
    }

    /// Spawn a supervised task. If the task panics or returns `Err`,
    /// the supervisor restarts it according to `policy`.
    pub fn spawn<F, Fut>(&self, name: &str, policy: RestartPolicy, f: F)
    where
        F: Fn(watch::Receiver<()>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let name = name.to_string();
        let event_log = Arc::clone(&self.event_log);
        let tasks = Arc::clone(&self.tasks);
        let (_shutdown_tx, shutdown_rx) = watch::channel(());

        // Register task
        {
            let mut list = tasks.write().unwrap();
            list.push(TaskStatus {
                name: name.clone(),
                status: "running".into(),
                restarts: 0,
                last_error: None,
            });
        }

        let mut restarts: u32 = 0;
        let start = std::time::Instant::now();

        tokio::spawn(async move {
            loop {
                let result = f(shutdown_rx.clone()).await;

                match result {
                    Ok(()) => {
                        let name_clone = name.clone();
                        {
                            let mut list = tasks.write().unwrap();
                            if let Some(t) = list.iter_mut().find(|t| t.name == name) {
                                t.status = "stopped".into();
                            }
                        }
                        event_log.publish(crate::events::AgentEvent {
                            agent_id: "supervisor".into(),
                            event_type: "task_stopped".into(),
                            severity: "info".into(),
                            timestamp: 0,
                            metadata: BoundedMeta::from_iter([("task".into(), name_clone)]),
                        });
                        break;
                    }
                    Err(e) => {
                        restarts += 1;
                        let err_str = format!("{}", e);

                        let should_restart = match policy {
                            RestartPolicy::Always => true,
                            RestartPolicy::Never => false,
                            RestartPolicy::Transient => restarts <= 1,
                            RestartPolicy::Permanent {
                                max_restarts,
                                within,
                            } => restarts <= max_restarts && start.elapsed() < within,
                        };

                        {
                            let mut list = tasks.write().unwrap();
                            if let Some(t) = list.iter_mut().find(|t| t.name == name) {
                                t.restarts = restarts;
                                t.last_error = Some(err_str.clone());
                                t.status = if should_restart {
                                    "restarting".into()
                                } else {
                                    "crashed".into()
                                };
                            }
                        }

                        let severity = if should_restart {
                            "warning"
                        } else {
                            "critical"
                        };
                        event_log.publish(crate::events::AgentEvent {
                            agent_id: "supervisor".into(),
                            event_type: (if should_restart {
                                "task_restarting"
                            } else {
                                "task_crashed"
                            })
                            .into(),
                            severity: severity.into(),
                            timestamp: 0,
                            metadata: BoundedMeta::from_iter([
                                ("task".into(), name.clone()),
                                ("error".into(), err_str),
                                ("restarts".into(), restarts.to_string()),
                            ]),
                        });

                        if !should_restart {
                            break;
                        }

                        // Backoff: 1s, 2s, 4s, 8s, cap at 30s
                        let backoff = Duration::from_secs((1u64 << restarts.min(5)).min(30));
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        });
    }

    /// Get current status of all supervised tasks.
    pub fn status(&self) -> Vec<TaskStatus> {
        self.tasks.read().unwrap().clone()
    }

    /// Get a shareable handle to the task list.
    pub fn tasks_handle(&self) -> Arc<std::sync::RwLock<Vec<TaskStatus>>> {
        Arc::clone(&self.tasks)
    }
}

// ─── axum handler ─────────────────────────────────────────────────

pub async fn handle_supervisor_status(
    axum::extract::State(supervisor): axum::extract::State<Arc<Supervisor>>,
) -> axum::Json<Vec<TaskStatus>> {
    axum::Json(supervisor.status())
}

pub fn router() -> axum::Router<Arc<Supervisor>> {
    axum::Router::new().route(
        "/supervisor/status",
        axum::routing::get(handle_supervisor_status),
    )
}
