use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub name: String,
    pub cadence_secs: u64,
    pub pattern: String,
    pub max_iterations: Option<usize>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub version: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConnector {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    pub id: String,
    pub role: String,
    pub model: String,
    pub instruction: String,
    pub max_turns: usize,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub score: f64,
    pub criteria_results: Vec<CriterionResult>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub outputs: Vec<String>,
    pub total_tokens: usize,
}

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
    pub fn to_prompt(&self) -> String {
        format!(
            r"# Loop Handoff — {}

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
",
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

    pub fn write_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_prompt())
    }
}

#[derive(Debug, Clone)]
pub struct LoopEngineConfig {
    pub name: String,
    pub max_iterations: usize,
    pub token_budget: Option<usize>,
    pub escalate_after_failures: usize,
    pub circuit_breaker_threshold: usize,
    pub evaluation_criteria: Vec<String>,
}

impl Default for LoopEngineConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            max_iterations: 10,
            token_budget: None,
            escalate_after_failures: 3,
            circuit_breaker_threshold: 5,
            evaluation_criteria: vec![
                "output produced".into(),
                "no errors".into(),
            ],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    #[error("Schedule not found: {0}")]
    ScheduleNotFound(String),
    #[error("Run not found: {0}")]
    RunNotFound(String),
    #[error("Max iterations reached ({0})")]
    MaxIterationsReached(usize),
    #[error("Token budget exceeded: {budget} budget, {spent} spent")]
    TokenBudgetExceeded { budget: usize, spent: usize },
    #[error("Circuit breaker is open — too many consecutive failures")]
    CircuitBreakerOpen,
    #[error("Agent error: {0}")]
    AgentError(String),
}
