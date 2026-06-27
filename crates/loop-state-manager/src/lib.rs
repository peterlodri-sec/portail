//! Loop State Manager — project-native loop state with HITL (Human-In-The-Loop)
//! integration for the DYAD communication layer.
//!
//! # Domains
//!
//! ```text
//! Box (Portail runtime)
//!   │
//!   ├── LoopStateManager  ←── tracks current state, version, next task
//!   │       │
//!   │       ├── get_status()      →  current phase + task
//!   │       ├── get_next_task()   →  what to do next (oneshot prompt)
//!   │       ├── update()          →  advance state
//!   │       └── query()           →  structured query over state history
//!   │
//!   └── DYAD surface (TUI / WebSocket / CLI)
//!           │
//!           ├── push state updates
//!           ├── receive human decisions
//!           └── HITL prompt → wait → decision → continue
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;
use uuid::Uuid;

// ── Core Types ───────────────────────────────────────────────────

/// A named phase in the development/release loop
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoopPhase {
    /// Planning — what to build next
    Planning,
    /// Implementation — coding / building
    Implementing,
    /// Testing — running tests, fixing failures
    Testing,
    /// Reviewing — code review, audit
    Reviewing,
    /// Deploying — release, ship
    Deploying,
    /// Monitoring — observe production, fix issues
    Monitoring,
    /// Blocked — waiting on human decision (HITL)
    Blocked(String),
    /// Done — loop complete
    Done,
}

impl std::fmt::Display for LoopPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoopPhase::Planning => write!(f, "planning"),
            LoopPhase::Implementing => write!(f, "implementing"),
            LoopPhase::Testing => write!(f, "testing"),
            LoopPhase::Reviewing => write!(f, "reviewing"),
            LoopPhase::Deploying => write!(f, "deploying"),
            LoopPhase::Monitoring => write!(f, "monitoring"),
            LoopPhase::Blocked(reason) => write!(f, "blocked({reason})"),
            LoopPhase::Done => write!(f, "done"),
        }
    }
}

/// A task within a loop phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopTask {
    pub id: String,
    pub phase: LoopPhase,
    pub description: String,
    pub prompt: Option<String>, // HITL prompt when human input needed
    pub status: TaskStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
    WaitingForHuman, // HITL — blocked on human decision
}

/// Human decision in response to a HITL prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecision {
    pub task_id: String,
    pub decision: String,
    pub reason: Option<String>,
    pub approved: bool,
}

/// The full loop state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    pub version: String,
    pub current_phase: LoopPhase,
    pub current_task: Option<LoopTask>,
    pub backlog: Vec<LoopTask>,
    pub history: Vec<LoopTask>,
    pub hitl_pending: Vec<LoopTask>, // tasks waiting for human
    pub updated_at: String,
}

// ── Error Type ───────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    #[error("No task available for phase {0}")]
    NoTaskAvailable(LoopPhase),
    #[error("Task {0} not found")]
    TaskNotFound(String),
    #[error("HITL timeout — human did not respond")]
    HitlTimeout,
    #[error("Invalid transition: {0} → {1}")]
    InvalidTransition(LoopPhase, LoopPhase),
}

// ── HITL Channel ─────────────────────────────────────────────────

/// A channel for waiting on a human decision
pub struct HitlChannel {
    tx: Mutex<Option<oneshot::Sender<HumanDecision>>>,
}

impl HitlChannel {
    pub fn new() -> (Self, oneshot::Receiver<HumanDecision>) {
        let (tx, rx) = oneshot::channel();
        (
            Self {
                tx: Mutex::new(Some(tx)),
            },
            rx,
        )
    }

    pub fn send_decision(&self, decision: HumanDecision) -> Result<(), HumanDecision> {
        let tx = self.tx.lock().unwrap().take();
        match tx {
            Some(tx) => tx.send(decision),
            None => Err(decision),
        }
    }
}

// ── Loop State Manager ───────────────────────────────────────────

/// The central loop state manager.
/// Thread-safe, Arc-able, wire into AppState.
pub struct LoopStateManager {
    state: Mutex<LoopState>,
}

impl LoopStateManager {
    pub fn new(version: &str) -> Self {
        Self {
            state: Mutex::new(LoopState {
                version: version.to_string(),
                current_phase: LoopPhase::Planning,
                current_task: None,
                backlog: Vec::new(),
                history: Vec::new(),
                hitl_pending: Vec::new(),
                updated_at: Utc::now().to_rfc3339(),
            }),
        }
    }

    /// Get a snapshot of the current loop state
    pub fn get_state(&self) -> LoopState {
        self.state.lock().unwrap().clone()
    }

    /// Get the next task for the current phase.
    /// Returns a oneshot prompt if human decision is needed.
    pub fn get_next_task(&self, phase: Option<LoopPhase>) -> Result<LoopTask, LoopError> {
        let mut state = self.state.lock().unwrap();
        let target_phase = phase.unwrap_or(state.current_phase.clone());

        // Find first pending task for this phase
        let pos = state
            .backlog
            .iter()
            .position(|t| t.phase == target_phase && t.status == TaskStatus::Pending);

        match pos {
            Some(idx) => {
                let mut task = state.backlog.remove(idx);
                task.status = TaskStatus::InProgress;
                state.current_task = Some(task.clone());
                state.updated_at = Utc::now().to_rfc3339();
                Ok(task)
            }
            None => Err(LoopError::NoTaskAvailable(target_phase)),
        }
    }

    /// Add a task to the backlog
    pub fn add_task(&self, phase: LoopPhase, description: &str, prompt: Option<&str>) -> LoopTask {
        let mut state = self.state.lock().unwrap();
        let task = LoopTask {
            id: Uuid::new_v4().to_string(),
            phase: phase.clone(),
            description: description.to_string(),
            prompt: prompt.map(|s| s.to_string()),
            status: TaskStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
            completed_at: None,
            metadata: HashMap::new(),
        };
        state.backlog.push(task.clone());
        state.updated_at = Utc::now().to_rfc3339();
        task
    }

    /// Mark a task as completed
    pub fn complete_task(&self, task_id: &str) -> Result<(), LoopError> {
        let mut state = self.state.lock().unwrap();

        // Check current task
        let is_current = state
            .current_task
            .as_ref()
            .map(|t| t.id == task_id)
            .unwrap_or(false);
        if is_current {
            let mut task = state.current_task.take().unwrap();
            task.status = TaskStatus::Completed;
            task.completed_at = Some(Utc::now().to_rfc3339());
            state.history.push(task);
            state.updated_at = Utc::now().to_rfc3339();
            return Ok(());
        }

        // Check backlog
        if let Some(pos) = state.backlog.iter().position(|t| t.id == task_id) {
            let mut task = state.backlog.remove(pos);
            task.status = TaskStatus::Completed;
            task.completed_at = Some(Utc::now().to_rfc3339());
            state.history.push(task);
            state.updated_at = Utc::now().to_rfc3339();
            return Ok(());
        }

        Err(LoopError::TaskNotFound(task_id.to_string()))
    }

    /// Set a task as waiting for human decision (HITL)
    pub fn wait_for_human(&self, task_id: &str) -> Result<(), LoopError> {
        let mut state = self.state.lock().unwrap();

        let should_block = state
            .current_task
            .as_ref()
            .map(|t| t.id == task_id)
            .unwrap_or(false);
        if should_block {
            let task = state.current_task.as_mut().unwrap();
            task.status = TaskStatus::WaitingForHuman;
            let clone = task.clone();
            state.hitl_pending.push(clone);
            state.current_phase = LoopPhase::Blocked("waiting for human".into());
            state.updated_at = Utc::now().to_rfc3339();
            return Ok(());
        }
        Err(LoopError::TaskNotFound(task_id.to_string()))
    }

    /// Receive a human decision and continue
    pub fn resolve_human(&self, decision: HumanDecision) -> Result<(), LoopError> {
        let mut state = self.state.lock().unwrap();

        let pos = state
            .hitl_pending
            .iter()
            .position(|t| t.id == decision.task_id);
        match pos {
            Some(idx) => {
                let mut task = state.hitl_pending.remove(idx);
                if decision.approved {
                    task.status = TaskStatus::InProgress;
                    state.current_task = Some(task.clone());
                    state.current_phase = LoopPhase::Implementing;
                } else {
                    task.status = TaskStatus::Failed(decision.reason.unwrap_or_default());
                    state.history.push(task);
                    state.current_task = None;
                }
                state.updated_at = Utc::now().to_rfc3339();
                Ok(())
            }
            None => Err(LoopError::TaskNotFound(decision.task_id)),
        }
    }

    /// Advance to the next phase
    pub fn advance_phase(&self, next: LoopPhase) -> Result<(), LoopError> {
        let mut state = self.state.lock().unwrap();
        state.current_phase = next;
        state.updated_at = Utc::now().to_rfc3339();
        Ok(())
    }

    /// Get a oneshot prompt for the current HITL task
    pub fn get_hitl_prompt(&self) -> Option<(String, String)> {
        let state = self.state.lock().unwrap();
        state.hitl_pending.first().map(|task| {
            let prompt = task
                .prompt
                .clone()
                .unwrap_or_else(|| format!("Approve task: {}", task.description));
            (task.id.clone(), prompt)
        })
    }

    /// Query state history
    pub fn query(&self, phase_filter: Option<LoopPhase>) -> Vec<LoopTask> {
        let state = self.state.lock().unwrap();
        match phase_filter {
            Some(phase) => state
                .history
                .iter()
                .filter(|t| t.phase == phase)
                .cloned()
                .collect(),
            None => state.history.clone(),
        }
    }

    /// Convert state to a JSON oneshot prompt for agents
    pub fn to_task_prompt(&self) -> String {
        let state = self.get_state();
        let current = state
            .current_task
            .as_ref()
            .map(|t| format!("Current: {} — {}", t.phase, t.description))
            .unwrap_or_else(|| "No current task".into());
        let backlog_count = state.backlog.len();
        let pending_hitl = state.hitl_pending.len();

        format!(
            "Loop State v{}\nPhase: {}\n{}\nBacklog: {} tasks\nHITL pending: {}",
            state.version, state.current_phase, current, backlog_count, pending_hitl
        )
    }
}

// ── DYAD types ───────────────────────────────────────────────────

/// Messages exchanged over the DYAD bidirectional channel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DyadMessage {
    /// Box → Human: state update
    StateUpdate(Box<LoopState>),
    /// Box → Human: HITL prompt, waiting for decision
    HitlPrompt { task_id: String, prompt: String },
    /// Human → Box: decision on HITL prompt
    HumanDecision(HumanDecision),
    /// Human → Box: command to execute
    Command {
        action: String,
        params: serde_json::Value,
    },
    /// Box → Human: command result
    CommandResult { success: bool, output: String },
    /// Either side: heartbeat / keepalive
    Ping,
    /// Either side: heartbeat response
    Pong,
}

/// The DYAD session state
pub struct DyadSession {
    pub session_id: String,
    pub connected_at: String,
    pub last_activity: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_state_initial() {
        let mgr = LoopStateManager::new("3.0.0");
        let state = mgr.get_state();
        assert_eq!(state.current_phase, LoopPhase::Planning);
        assert_eq!(state.version, "3.0.0");
    }

    #[test]
    fn test_add_and_complete_task() {
        let mgr = LoopStateManager::new("1.0.0");
        mgr.add_task(LoopPhase::Implementing, "Build the feature", None);
        let task = mgr.get_next_task(Some(LoopPhase::Implementing)).unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        mgr.complete_task(&task.id).unwrap();
        let state = mgr.get_state();
        assert!(state.current_task.is_none());
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn test_hitl_flow() {
        let mgr = LoopStateManager::new("1.0.0");
        mgr.add_task(
            LoopPhase::Reviewing,
            "Review PR #42",
            Some("Approve the changes? (yes/no)"),
        );
        let task = mgr.get_next_task(Some(LoopPhase::Reviewing)).unwrap();
        mgr.wait_for_human(&task.id).unwrap();

        let (task_id, prompt) = mgr.get_hitl_prompt().unwrap();
        assert_eq!(task_id, task.id);
        assert!(prompt.contains("Approve"));

        mgr.resolve_human(HumanDecision {
            task_id: task.id.clone(),
            decision: "yes".into(),
            reason: Some("LGTM".into()),
            approved: true,
        })
        .unwrap();

        let state = mgr.get_state();
        assert_eq!(state.current_phase, LoopPhase::Implementing);
    }

    #[test]
    fn test_backlog_order() {
        let mgr = LoopStateManager::new("1.0.0");
        mgr.add_task(LoopPhase::Implementing, "Task A", None);
        mgr.add_task(LoopPhase::Implementing, "Task B", None);
        let a = mgr.get_next_task(Some(LoopPhase::Implementing)).unwrap();
        assert_eq!(a.description, "Task A");
        mgr.complete_task(&a.id).unwrap();
        let b = mgr.get_next_task(Some(LoopPhase::Implementing)).unwrap();
        assert_eq!(b.description, "Task B");
    }

    #[test]
    fn test_dyad_message_serde() {
        let msg = DyadMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, "{\"type\":\"Ping\"}");
        let back: DyadMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DyadMessage::Ping));
    }

    #[test]
    fn test_task_prompt_format() {
        let mgr = LoopStateManager::new("3.0.0");
        mgr.add_task(LoopPhase::Implementing, "Write tests", None);
        let prompt = mgr.to_task_prompt();
        assert!(prompt.contains("Loop State"));
        assert!(prompt.contains("3.0.0"));
    }
}
