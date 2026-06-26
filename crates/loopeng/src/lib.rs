//! Loop Engineering — Rust-native primitives + engine.
//!
//! ## Anatomy of a Loop
//!
//! ```text
//!   plan ──→ execute ──→ evaluate ──→ decide
//!     ↑                                   │
//!     └───────────────────────────────────┘
//! ```
//!
//! ## Five Building Blocks + Memory
//!
//! | Primitive | Role |
//! |-----------|------|
//! | Automation / Schedule | Trigger loops on a cadence |
//! | Worktree | Isolated parallel execution |
//! | Skill | Persistent project knowledge |
//! | Plugin / Connector | Reach into real tools (MCP) |
//! | Sub-agent | Maker / checker split |
//! | Memory / State | Durable spine outside any conversation |
//!
//! ## _next-prompt
//!
//! A handoff prompt generated at the end of each session. Fresh agents
//! read `_next-prompt` to continue where the last session left off.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ── Phase Primitives ────────────────────────────────────────────

/// The four phases of every loop
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoopPhase {
    Plan,
    Execute,
    Evaluate,
    Decide,
}

impl std::fmt::Display for LoopPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoopPhase::Plan => write!(f, "plan"),
            LoopPhase::Execute => write!(f, "execute"),
            LoopPhase::Evaluate => write!(f, "evaluate"),
            LoopPhase::Decide => write!(f, "decide"),
        }
    }
}

// ── Five Building Blocks ────────────────────────────────────────

/// 1. Automation / Schedule — trigger on cadence or event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub name: String,
    pub cadence_secs: u64,
    pub pattern: String,
    pub max_iterations: Option<usize>,
    pub enabled: bool,
}

/// 2. Worktree — isolated parallel execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub id: String,
    pub branch: String,
    pub path: String,
    pub created_at: String,
    pub status: WorktreeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorktreeStatus {
    Active,
    Completed,
    Failed(String),
}

/// 3. Skill — persistent project knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub version: String,
    pub tags: Vec<String>,
}

/// 4. Plugin / Connector — MCP server binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConnector {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub enabled: bool,
}

/// 5. Sub-agent — delegated execution with role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    pub id: String,
    pub role: String,        // "maker", "checker", "researcher"
    pub model: String,
    pub instruction: String,
    pub max_turns: usize,
}

/// + Memory / State — durable spine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopMemory {
    pub entries: Vec<MemoryEntry>,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub tags: Vec<String>,
}

// ── Loop Run ────────────────────────────────────────────────────

/// One iteration of a loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRun {
    pub id: String,
    pub schedule_name: String,
    pub phase: LoopPhase,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: RunStatus,
    pub output: Option<String>,
    pub artifacts: Vec<String>,
    pub token_cost: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RunStatus {
    Running,
    Passed,
    Failed(String),
    Skipped,
    Escalated,
}

// ── Council Decision ─────────────────────────────────────────────

/// Council vote result — SHIP, ITERATE, ESCALATE
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CouncilDecision {
    Ship,
    Iterate { reason: String },
    Escalate { reason: String, context: String },
}

impl std::fmt::Display for CouncilDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CouncilDecision::Ship => write!(f, "SHIP"),
            CouncilDecision::Iterate { reason } => write!(f, "ITERATE: {reason}"),
            CouncilDecision::Escalate { reason, .. } => write!(f, "ESCALATE: {reason}"),
        }
    }
}

// ── _next-prompt ─────────────────────────────────────────────────

/// A handoff prompt for the next session. Generated at end of each loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextPrompt {
    pub session_id: String,
    pub generated_at: String,
    pub loop_name: String,
    pub current_phase: String,
    pub last_run: String,
    pub status: String,
    pub next_action: String,
    pub context: String,
    pub artifacts: Vec<String>,
    pub token_spent: Option<usize>,
}

impl NextPrompt {
    /// Format as a markdown prompt that a fresh agent can consume
    pub fn to_prompt(&self) -> String {
        format!(
            r#"# 🔄 Loop Handoff — {}

## State
- **Phase:** {}
- **Last run:** {}
- **Status:** {}

## Context
{}

## Next Action
{}

## Artifacts
{}

## Command
```bash
# Continue this loop
portail loop next {}
```
"#,
            self.loop_name,
            self.current_phase,
            self.last_run,
            self.status,
            self.context,
            self.next_action,
            self.artifacts.join("\n"),
            self.loop_name,
        )
    }

    /// Write to `_next-prompt.md` in the project root
    pub fn write_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_prompt())
    }
}

// ── Loop Engine ─────────────────────────────────────────────────

/// The core loop engine. Runs plan → execute → evaluate → decide.
pub struct LoopEngine {
    schedules: Vec<Schedule>,
    skills: Vec<Skill>,
    plugins: Vec<PluginConnector>,
    sub_agents: Vec<SubAgent>,
    memory: LoopMemory,
    runs: Vec<LoopRun>,
    config: LoopEngineConfig,
}

#[derive(Debug, Clone)]
pub struct LoopEngineConfig {
    pub name: String,
    pub max_iterations: usize,
    pub token_budget: Option<usize>,
    pub escalate_after_failures: usize,
}

impl Default for LoopEngineConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            max_iterations: 10,
            token_budget: None,
            escalate_after_failures: 3,
        }
    }
}

impl LoopEngine {
    pub fn new(config: LoopEngineConfig) -> Self {
        Self {
            schedules: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            sub_agents: Vec::new(),
            memory: LoopMemory { entries: Vec::new(), max_entries: 100 },
            runs: Vec::new(),
            config,
        }
    }

    // ── Building block registration ──

    pub fn add_schedule(&mut self, schedule: Schedule) {
        self.schedules.push(schedule);
    }

    pub fn add_skill(&mut self, skill: Skill) {
        self.skills.push(skill);
    }

    pub fn add_plugin(&mut self, plugin: PluginConnector) {
        self.plugins.push(plugin);
    }

    pub fn add_sub_agent(&mut self, agent: SubAgent) {
        self.sub_agents.push(agent);
    }

    pub fn remember(&mut self, key: &str, value: &str, tags: Vec<String>) {
        if self.memory.entries.len() >= self.memory.max_entries {
            self.memory.entries.remove(0);
        }
        self.memory.entries.push(MemoryEntry {
            key: key.to_string(),
            value: value.to_string(),
            created_at: Utc::now().to_rfc3339(),
            tags,
        });
    }

    pub fn recall(&self, key: &str) -> Option<&MemoryEntry> {
        self.memory.entries.iter().rev().find(|e| e.key == key)
    }

    pub fn recall_by_tag(&self, tag: &str) -> Vec<&MemoryEntry> {
        self.memory.entries.iter().rev().filter(|e| e.tags.contains(&tag.to_string())).collect()
    }

    // ── Loop execution ──

    /// Run one iteration of the loop (plan → execute → evaluate → decide)
    pub async fn run_iteration(&mut self, schedule_name: &str) -> Result<LoopRun, LoopError> {
        let schedule = self.schedules.iter().find(|s| s.name == schedule_name)
            .ok_or_else(|| LoopError::ScheduleNotFound(schedule_name.to_string()))?;

        let run_id = Uuid::new_v4().to_string();
        let mut run = LoopRun {
            id: run_id.clone(),
            schedule_name: schedule_name.to_string(),
            phase: LoopPhase::Plan,
            started_at: Utc::now().to_rfc3339(),
            completed_at: None,
            status: RunStatus::Running,
            output: None,
            artifacts: Vec::new(),
            token_cost: None,
            error: None,
        };

        // Phase 1: Plan
        run.phase = LoopPhase::Plan;
        let plan = self.execute_plan(schedule).await?;
        run.artifacts.push(plan);

        // Phase 2: Execute
        run.phase = LoopPhase::Execute;
        let result = self.execute(schedule, &run).await?;
        run.output = Some(result.clone());

        // Phase 3: Evaluate
        run.phase = LoopPhase::Evaluate;
        let evaluation = self.evaluate(schedule, &result).await?;

        // Phase 4: Decide
        run.phase = LoopPhase::Decide;
        let decision = self.decide(schedule, &evaluation).await?;

        run.completed_at = Some(Utc::now().to_rfc3339());
        run.status = match decision {
            CouncilDecision::Ship => RunStatus::Passed,
            CouncilDecision::Iterate { ref reason } => {
                self.remember("last_iteration_reason", reason, vec!["loop".into()]);
                RunStatus::Skipped
            }
            CouncilDecision::Escalate { ref reason, .. } => {
                self.remember("last_escalation", reason, vec!["escalation".into()]);
                RunStatus::Escalated
            }
        };

        self.runs.push(run.clone());
        Ok(run)
    }

    async fn execute_plan(&self, schedule: &Schedule) -> Result<String, LoopError> {
        // Gather context from memory + skills
        let skills_summary: Vec<String> = self.skills.iter()
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect();
        let recent_memory: Vec<String> = self.memory.entries.iter().rev().take(5)
            .map(|e| format!("  {} = {}", e.key, e.value))
            .collect();

        Ok(format!(
            "Plan for {} (pattern: {})\nSkills available:\n{}\nRecent memory:\n{}\nMax iterations: {}",
            schedule.name, schedule.pattern,
            skills_summary.join("\n"),
            recent_memory.join("\n"),
            self.config.max_iterations,
        ))
    }

    async fn execute(&self, schedule: &Schedule, run: &LoopRun) -> Result<String, LoopError> {
        let sub_agents: Vec<String> = self.sub_agents.iter()
            .map(|a| format!("  {} ({}) — {}", a.role, a.model, a.instruction))
            .collect();

        Ok(format!(
            "Executing {} (run {})\nSub-agents:\n{}\n",
            schedule.name, run.id,
            sub_agents.join("\n"),
        ))
    }

    async fn evaluate(&self, _schedule: &Schedule, result: &str) -> Result<String, LoopError> {
        Ok(format!("Evaluation of result ({} chars): needs human review", result.len()))
    }

    async fn decide(&self, _schedule: &Schedule, evaluation: &str) -> Result<CouncilDecision, LoopError> {
        // Default: escalate to human after evaluation
        Ok(CouncilDecision::Escalate {
            reason: "Evaluation complete, human decision required".into(),
            context: evaluation.to_string(),
        })
    }

    // ── _next-prompt generation ──

    /// Generate a _next-prompt for handoff to a fresh session
    pub fn generate_next_prompt(&self, loop_name: &str) -> NextPrompt {
        let last_run = self.runs.last()
            .map(|r| format!("{} — {}", r.phase, r.id))
            .unwrap_or_else(|| "no runs yet".into());

        let current_phase = self.runs.last()
            .map(|r| r.phase.to_string())
            .unwrap_or_else(|| "plan".into());

        let status = self.runs.last()
            .map(|r| format!("{:?}", r.status))
            .unwrap_or_else(|| "new".into());

        let context = self.memory.entries.iter().rev().take(3)
            .map(|e| format!("- {}: {}", e.key, e.value))
            .collect::<Vec<_>>()
            .join("\n");

        let total_tokens: Option<usize> = self.runs.iter()
            .filter_map(|r| r.token_cost)
            .reduce(|a, b| a + b);

        NextPrompt {
            session_id: Uuid::new_v4().to_string(),
            generated_at: Utc::now().to_rfc3339(),
            loop_name: loop_name.to_string(),
            current_phase,
            last_run,
            status,
            next_action: format!("Continue loop {} — run next iteration", loop_name),
            context: if context.is_empty() { "No context stored yet".into() } else { context },
            artifacts: self.runs.iter().flat_map(|r| r.artifacts.clone()).collect(),
            token_spent: total_tokens,
        }
    }

    // ── Query ──

    pub fn schedules(&self) -> &[Schedule] { &self.schedules }
    pub fn skills(&self) -> &[Skill] { &self.skills }
    pub fn runs(&self) -> &[LoopRun] { &self.runs }
    pub fn memory_entries(&self) -> &[MemoryEntry] { &self.memory.entries }
    pub fn sub_agents(&self) -> &[SubAgent] { &self.sub_agents }
    pub fn config(&self) -> &LoopEngineConfig { &self.config }
}

// ── Error ───────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    #[error("Schedule not found: {0}")]
    ScheduleNotFound(String),
    #[error("Max iterations reached ({0})")]
    MaxIterationsReached(usize),
    #[error("Agent error: {0}")]
    AgentError(String),
}

// ── Thread-safe wrapper ─────────────────────────────────────────

pub struct SharedLoopEngine {
    inner: Mutex<LoopEngine>,
}

impl SharedLoopEngine {
    pub fn new(config: LoopEngineConfig) -> Self {
        Self { inner: Mutex::new(LoopEngine::new(config)) }
    }

    pub fn with_engine<F, R>(&self, f: F) -> R where F: FnOnce(&mut LoopEngine) -> R {
        let mut engine = self.inner.lock().unwrap();
        f(&mut engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_phase_display() {
        assert_eq!(LoopPhase::Plan.to_string(), "plan");
        assert_eq!(LoopPhase::Execute.to_string(), "execute");
    }

    #[test]
    fn test_council_display() {
        assert_eq!(CouncilDecision::Ship.to_string(), "SHIP");
        let iter = CouncilDecision::Iterate { reason: "needs more data".into() };
        assert_eq!(iter.to_string(), "ITERATE: needs more data");
    }

    #[test]
    fn test_engine_add_schedule() {
        let mut engine = LoopEngine::new(LoopEngineConfig::default());
        engine.add_schedule(Schedule {
            name: "daily-triage".into(),
            cadence_secs: 3600,
            pattern: "daily-triage".into(),
            max_iterations: None,
            enabled: true,
        });
        assert_eq!(engine.schedules().len(), 1);
    }

    #[test]
    fn test_memory_recall() {
        let mut engine = LoopEngine::new(LoopEngineConfig::default());
        engine.remember("last_result", "all tests passed", vec!["test".into()]);
        engine.remember("last_model", "sonnet-4", vec!["model".into()]);

        let r = engine.recall("last_result");
        assert!(r.is_some());
        assert_eq!(r.unwrap().value, "all tests passed");

        let tagged = engine.recall_by_tag("model");
        assert_eq!(tagged.len(), 1);
    }

    #[test]
    fn test_next_prompt_format() {
        let mut engine = LoopEngine::new(LoopEngineConfig::default());
        engine.add_schedule(Schedule {
            name: "test-loop".into(),
            cadence_secs: 60,
            pattern: "test".into(),
            max_iterations: Some(5),
            enabled: true,
        });
        engine.remember("key_result", "42", vec!["number".into()]);

        let prompt = engine.generate_next_prompt("test-loop");
        let markdown = prompt.to_prompt();
        assert!(markdown.contains("Loop Handoff"));
        assert!(markdown.contains("test-loop"));
        assert!(markdown.contains("42"));
    }

    #[test]
    fn test_add_sub_agent() {
        let mut engine = LoopEngine::new(LoopEngineConfig::default());
        engine.add_sub_agent(SubAgent {
            id: "checker-1".into(),
            role: "checker".into(),
            model: "sonnet-4".into(),
            instruction: "Verify all outputs".into(),
            max_turns: 3,
        });
        assert_eq!(engine.sub_agents().len(), 1);
    }

    #[test]
    fn test_shared_engine() {
        let shared = SharedLoopEngine::new(LoopEngineConfig::default());
        shared.with_engine(|e| {
            e.add_schedule(Schedule {
                name: "shared-test".into(),
                cadence_secs: 300,
                pattern: "test".into(),
                max_iterations: None,
                enabled: true,
            });
        });
        let count = shared.with_engine(|e| e.schedules().len());
        assert_eq!(count, 1);
    }
}
