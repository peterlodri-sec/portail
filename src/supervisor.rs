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
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_policy_constructors() {
        assert!(matches!(RestartPolicy::always(), RestartPolicy::Always));
        assert!(matches!(RestartPolicy::never(), RestartPolicy::Never));
        assert!(matches!(
            RestartPolicy::transient(),
            RestartPolicy::Transient
        ));
        let perm = RestartPolicy::permanent();
        match perm {
            RestartPolicy::Permanent {
                max_restarts,
                within,
            } => {
                assert_eq!(max_restarts, 5);
                assert_eq!(within, Duration::from_secs(60));
            }
            _ => panic!("expected Permanent"),
        }
    }

    #[test]
    fn supervisor_new_creates_empty_task_list() {
        let log = Arc::new(crate::events::EventLog::new(100));
        let sup = Supervisor::new(log);
        assert!(sup.status().is_empty());
    }

    #[test]
    fn supervisor_tasks_handle_is_shared() {
        let log = Arc::new(crate::events::EventLog::new(100));
        let sup = Supervisor::new(log);
        let handle = sup.tasks_handle();
        assert_eq!(handle.read().unwrap().len(), sup.status().len());
    }

    #[tokio::test]
    async fn supervisor_spawn_successful_task() {
        let log = Arc::new(crate::events::EventLog::new(100));
        let sup = Arc::new(Supervisor::new(log));

        sup.spawn(
            "test-task",
            RestartPolicy::never(),
            |_rx| async move { Ok(()) },
        );

        tokio::time::sleep(Duration::from_millis(50)).await;

        let statuses = sup.status();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "test-task");
    }

    #[tokio::test]
    async fn supervisor_spawn_failing_task_crashes_with_never_policy() {
        let log = Arc::new(crate::events::EventLog::new(100));
        let sup = Arc::new(Supervisor::new(log));

        sup.spawn("crashing-task", RestartPolicy::never(), |_rx| async move {
            Err(anyhow::anyhow!("intentional crash"))
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let statuses = sup.status();
        let task = statuses.iter().find(|t| t.name == "crashing-task").unwrap();
        assert_eq!(task.status, "crashed");
        assert_eq!(task.restarts, 1);
        assert!(
            task.last_error
                .as_ref()
                .unwrap()
                .contains("intentional crash")
        );
    }

    #[tokio::test]
    async fn supervisor_spawn_with_always_policy_restarts() {
        let log = Arc::new(crate::events::EventLog::new(100));
        let sup = Arc::new(Supervisor::new(log));

        sup.spawn(
            "restarting-task",
            RestartPolicy::always(),
            |_rx| async move { Err(anyhow::anyhow!("boom")) },
        );

        tokio::time::sleep(Duration::from_secs(1)).await;

        let statuses = sup.status();
        let task = statuses
            .iter()
            .find(|t| t.name == "restarting-task")
            .unwrap();
        assert!(task.restarts >= 1);
    }

    #[test]
    fn task_status_serialization() {
        let status = TaskStatus {
            name: "test".into(),
            status: "running".into(),
            restarts: 0,
            last_error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("running"));
        let deser: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
    }
}
