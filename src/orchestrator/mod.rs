use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, mpsc};

// ── Fleet Anchor ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub protocol: AgentProtocol,
    pub capabilities: Vec<String>,
    pub connected_at: String,
    pub last_heartbeat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentProtocol {
    A2A,
    A2C,
    Mcp,
    Stdio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_agents: Vec<String>,
    pub subtask_count: usize,
}

// ── Agent Config & SubTask Schemas ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub role: String,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub temperature: f64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            role: "assistant".into(),
            model: "openrouter/openai/gpt-4o".into(),
            system_prompt: String::new(),
            max_turns: 5,
            temperature: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub description: String,
    pub context_files: Vec<String>,
    pub success_criteria: Vec<String>,
    pub agent_config: AgentConfig,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskResult {
    pub task_id: String,
    pub status: SubTaskStatus,
    pub output: Option<String>,
    pub files_changed: Vec<String>,
    pub token_cost: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubTaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorGoal {
    pub id: String,
    pub workflow: String,
    pub description: String,
    pub target_files: Vec<String>,
    pub created_at: String,
    pub subtasks: Vec<SubTask>,
}

impl OrchestratorGoal {
    pub fn deep_research(query: &str) -> Self {
        Self {
            id: format!(
                "goal-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ),
            workflow: "deep-research".into(),
            description: format!("Deep research: {query}"),
            target_files: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            subtasks: vec![SubTask {
                id: "research".into(),
                description: format!("Research: {query}"),
                context_files: vec![],
                success_criteria: vec!["find sources".into(), "summarize".into()],
                agent_config: AgentConfig {
                    id: "researcher".into(),
                    role: "researcher".into(),
                    model: "openrouter/openai/gpt-4o".into(),
                    system_prompt:
                        "Research the topic deeply. Find sources, summarize, provide citations."
                            .into(),
                    max_turns: 5,
                    temperature: 0.7,
                },
                dependencies: vec![],
            }],
        }
    }

    pub fn coding_task(task: &str, files: Vec<String>) -> Self {
        Self {
            id: format!(
                "goal-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ),
            workflow: "coding".into(),
            description: format!("Coding: {task}"),
            target_files: files.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            subtasks: vec![
                SubTask {
                    id: "plan".into(),
                    description: format!("Plan: {task}"),
                    context_files: files.clone(),
                    success_criteria: vec!["analyze requirements".into()],
                    agent_config: AgentConfig {
                        id: "architect".into(),
                        role: "planner".into(),
                        model: "openrouter/anthropic/claude-sonnet-4".into(),
                        system_prompt: "Analyze the task and produce an implementation plan."
                            .into(),
                        max_turns: 3,
                        temperature: 0.5,
                    },
                    dependencies: vec![],
                },
                SubTask {
                    id: "implement".into(),
                    description: format!("Implement: {task}"),
                    context_files: files,
                    success_criteria: vec!["write code".into(), "verify".into()],
                    agent_config: AgentConfig {
                        id: "builder".into(),
                        role: "maker".into(),
                        model: "openrouter/anthropic/claude-sonnet-4".into(),
                        system_prompt: "Write production-quality Rust code.".into(),
                        max_turns: 8,
                        temperature: 0.3,
                    },
                    dependencies: vec!["plan".into()],
                },
            ],
        }
    }

    pub fn review_task(changes: &str) -> Self {
        Self {
            id: format!(
                "goal-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ),
            workflow: "review".into(),
            description: format!("Code review: {changes}"),
            target_files: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            subtasks: vec![SubTask {
                id: "reviewer".into(),
                description: format!("Review: {changes}"),
                context_files: vec![],
                success_criteria: vec!["check correctness".into(), "check style".into()],
                agent_config: AgentConfig {
                    id: "reviewer".into(),
                    role: "checker".into(),
                    model: "openrouter/anthropic/claude-sonnet-4".into(),
                    system_prompt: "Review code for correctness, style, and security issues."
                        .into(),
                    max_turns: 3,
                    temperature: 0.2,
                },
                dependencies: vec![],
            }],
        }
    }
}

// ── Event Stream ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AgentEvent {
    AgentStarted {
        agent_id: String,
        task: String,
    },
    AgentProgress {
        agent_id: String,
        message: String,
    },
    AgentCompleted {
        agent_id: String,
        result: SubTaskResult,
    },
    AgentFailed {
        agent_id: String,
        error: String,
    },
    OrchestratorLog {
        message: String,
    },
    GoalComplete {
        goal_id: String,
        workflow: String,
        success: bool,
    },
    AgentCheckedIn {
        registration: AgentRegistration,
    },
    AgentCheckedOut {
        agent_id: String,
    },
}

pub type EventSender = mpsc::UnboundedSender<AgentEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<AgentEvent>;

pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

// ── Fleet Registry ──────────────────────────────────────────────

pub struct FleetRegistry {
    agents: RwLock<HashMap<String, AgentRegistration>>,
    workflows: Vec<WorkflowTemplate>,
}

impl Default for FleetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FleetRegistry {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            workflows: vec![
                WorkflowTemplate {
                    id: "deep-research".into(),
                    name: "Deep Research".into(),
                    description: "Multi-source research with synthesis".into(),
                    default_agents: vec!["researcher".into()],
                    subtask_count: 1,
                },
                WorkflowTemplate {
                    id: "coding".into(),
                    name: "Coding Task".into(),
                    description: "Plan + implement + verify".into(),
                    default_agents: vec!["architect".into(), "builder".into()],
                    subtask_count: 2,
                },
                WorkflowTemplate {
                    id: "review".into(),
                    name: "Code Review".into(),
                    description: "Automated code review".into(),
                    default_agents: vec!["reviewer".into()],
                    subtask_count: 1,
                },
            ],
        }
    }

    pub async fn check_in(&self, reg: AgentRegistration) {
        let mut agents = self.agents.write().await;
        agents.insert(reg.id.clone(), reg);
    }

    pub async fn check_out(&self, agent_id: &str) {
        let mut agents = self.agents.write().await;
        agents.remove(agent_id);
    }

    pub async fn list_agents(&self) -> Vec<AgentRegistration> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    pub fn workflows(&self) -> &[WorkflowTemplate] {
        &self.workflows
    }
}

// ── Pluggable Tool Trait ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    pub session_id: String,
    pub agent_id: String,
    pub working_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

#[async_trait]
pub trait AgentTool: Send + Sync {
    fn metadata(&self) -> ToolMetadata;
    async fn call(&self, args: serde_json::Value, ctx: ToolContext) -> ToolOutput;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        let name = tool.metadata().name;
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<ToolMetadata> {
        self.tools.values().map(|t| t.metadata()).collect()
    }
}

// ── System State ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SystemState {
    pub goal: Option<OrchestratorGoal>,
    pub active_agents: Vec<ActiveAgent>,
    pub completed: Vec<SubTaskResult>,
    pub log_messages: Vec<String>,
    pub started_at: Instant,
    pub total_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct ActiveAgent {
    pub agent_id: String,
    pub task: String,
    pub progress: String,
    pub started_at: Instant,
}

impl Default for SystemState {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemState {
    pub fn new() -> Self {
        Self {
            goal: None,
            active_agents: Vec::new(),
            completed: Vec::new(),
            log_messages: Vec::new(),
            started_at: Instant::now(),
            total_tokens: 0,
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

// ── Fan-Out Engine ──────────────────────────────────────────────

pub struct FanOutEngine {
    sender: EventSender,
}

impl FanOutEngine {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }

    pub async fn execute(&self, goal: OrchestratorGoal) -> Vec<SubTaskResult> {
        let mut results = Vec::new();
        self.log(
            "goal",
            &format!("Starting '{}': {}", goal.workflow, goal.description),
        );

        for task in &goal.subtasks {
            self.send(AgentEvent::AgentStarted {
                agent_id: task.agent_config.id.clone(),
                task: task.description.clone(),
            });

            let result = self.run_subtask(task).await;
            match &result.status {
                SubTaskStatus::Completed => {
                    self.send(AgentEvent::AgentCompleted {
                        agent_id: task.agent_config.id.clone(),
                        result: result.clone(),
                    });
                }
                SubTaskStatus::Failed(e) => {
                    self.send(AgentEvent::AgentFailed {
                        agent_id: task.agent_config.id.clone(),
                        error: e.clone(),
                    });
                }
                _ => {}
            }
            results.push(result);
        }

        let success = results.iter().all(|r| r.status == SubTaskStatus::Completed);
        self.send(AgentEvent::GoalComplete {
            goal_id: goal.id,
            workflow: goal.workflow,
            success,
        });

        results
    }

    async fn run_subtask(&self, task: &SubTask) -> SubTaskResult {
        let start = Instant::now();
        let context = task.context_files.join(", ");

        self.send(AgentEvent::AgentProgress {
            agent_id: task.agent_config.id.clone(),
            message: format!(
                "Context: [{}]",
                if context.is_empty() { "none" } else { &context }
            ),
        });

        let steps = task.agent_config.max_turns as u32;
        for step in 0..steps {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            tokio::task::yield_now().await;
            let criterion = task
                .success_criteria
                .get(step as usize)
                .cloned()
                .unwrap_or_else(|| "working".into());
            self.send(AgentEvent::AgentProgress {
                agent_id: task.agent_config.id.clone(),
                message: format!("Step {}/{} — {criterion}", step + 1, steps),
            });
        }

        SubTaskResult {
            task_id: task.id.clone(),
            status: SubTaskStatus::Completed,
            output: Some(format!(
                "Agent '{}' completed: {}",
                task.agent_config.id, task.description
            )),
            files_changed: task.context_files.clone(),
            token_cost: (steps as usize) * 500,
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        }
    }

    fn log(&self, tag: &str, msg: &str) {
        self.send(AgentEvent::OrchestratorLog {
            message: format!("[{tag}] {msg}"),
        });
    }

    fn send(&self, event: AgentEvent) {
        let _ = self.sender.send(event);
    }
}
