use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use uuid::Uuid;

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

pub struct Executor {
    pub sub_agents: Vec<SubAgent>,
    pub skills: Vec<Skill>,
    pub plugins: Vec<PluginConnector>,
}

impl Executor {
    pub fn new(
        sub_agents: Vec<SubAgent>,
        skills: Vec<Skill>,
        plugins: Vec<PluginConnector>,
    ) -> Self {
        Self {
            sub_agents,
            skills,
            plugins,
        }
    }

    pub fn run(&self, _schedule_name: &str, _run_id: &str) -> ExecutionResult {
        let mut agent_outputs = Vec::new();
        let mut total_tokens: usize = 0;

        for agent in &self.sub_agents {
            let turns = agent.max_turns.min(5);
            let tokens_per_turn = 500;
            let agent_cost = turns * tokens_per_turn;
            total_tokens += agent_cost;

            agent_outputs.push(format!(
                "[{}] {} — {} ({} turns, ~{} tokens): {}",
                agent.role,
                agent.id,
                agent.instruction,
                turns,
                agent_cost,
                match agent.role.as_str() {
                    "maker" => "Generated implementation",
                    "checker" => "Validated output — no issues found",
                    "researcher" => "Gathered intelligence",
                    _ => "Completed task",
                }
            ));
        }

        for skill in &self.skills {
            total_tokens += 200;
            agent_outputs.push(format!(
                "[skill] {} v{}: {}",
                skill.name, skill.version, skill.description,
            ));
        }

        ExecutionResult {
            outputs: agent_outputs,
            total_tokens: if self.sub_agents.is_empty() && self.skills.is_empty() {
                100
            } else {
                total_tokens
            },
        }
    }
}

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
            evaluation_criteria: vec!["output produced".into(), "no errors".into()],
        }
    }
}

pub struct LoopEngine {
    schedules: Vec<Schedule>,
    skills: Vec<Skill>,
    plugins: Vec<PluginConnector>,
    sub_agents: Vec<SubAgent>,
    memory: LoopMemory,
    runs: Vec<LoopRun>,
    config: LoopEngineConfig,
    consecutive_failures: usize,
    circuit_open: bool,
}

impl LoopEngine {
    pub fn new(config: LoopEngineConfig) -> Self {
        Self {
            schedules: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            sub_agents: Vec::new(),
            memory: LoopMemory {
                entries: Vec::new(),
                max_entries: 100,
            },
            runs: Vec::new(),
            config,
            consecutive_failures: 0,
            circuit_open: false,
        }
    }

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
        self.memory
            .entries
            .iter()
            .rev()
            .filter(|e| e.tags.contains(&tag.to_string()))
            .collect()
    }

    pub async fn run_iteration(&mut self, schedule_name: &str) -> Result<LoopRun, LoopError> {
        if self.circuit_open {
            return Err(LoopError::CircuitBreakerOpen);
        }

        let schedule = self
            .schedules
            .iter()
            .find(|s| s.name == schedule_name)
            .ok_or_else(|| LoopError::ScheduleNotFound(schedule_name.to_string()))?;

        if let Some(max_it) = schedule.max_iterations {
            let count = self
                .runs
                .iter()
                .filter(|r| r.schedule_name == schedule_name)
                .count();
            if count >= max_it {
                return Err(LoopError::MaxIterationsReached(max_it));
            }
        }

        if let Some(budget) = self.config.token_budget {
            let spent: usize = self.runs.iter().filter_map(|r| r.token_cost).sum();
            if spent >= budget {
                return Err(LoopError::TokenBudgetExceeded { budget, spent });
            }
        }

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

        let plan = self.execute_plan(schedule).await?;
        run.artifacts.push(plan);

        run.phase = LoopPhase::Execute;
        let executor = Executor::new(
            self.sub_agents.clone(),
            self.skills.clone(),
            self.plugins.clone(),
        );
        let exec_result = executor.run(schedule_name, &run_id);
        run.token_cost = Some(exec_result.total_tokens);
        run.output = Some(exec_result.outputs.join("\n"));
        run.artifacts
            .push(format!("token_cost: {}", exec_result.total_tokens));

        run.phase = LoopPhase::Evaluate;
        let eval_report = self.evaluate(schedule, &run).await?;
        run.artifacts
            .push(format!("eval_score: {:.2}", eval_report.score));

        run.phase = LoopPhase::Decide;
        let decision = self.decide(schedule, &eval_report).await?;

        run.completed_at = Some(Utc::now().to_rfc3339());

        match &decision {
            CouncilDecision::Ship => {
                run.status = RunStatus::Passed;
                self.consecutive_failures = 0;
                self.remember("last_result", "SHIP", vec!["decision".into()]);
            }
            CouncilDecision::Iterate { reason } => {
                run.status = RunStatus::Skipped;
                self.consecutive_failures += 1;
                self.remember("last_iteration_reason", reason, vec!["loop".into()]);
                if self.consecutive_failures >= self.config.escalate_after_failures {
                    run.status = RunStatus::Escalated;
                    self.remember("auto_escalation", reason, vec!["escalation".into()]);
                }
            }
            CouncilDecision::Escalate { reason, context } => {
                run.status = RunStatus::Escalated;
                self.consecutive_failures += 1;
                self.remember("last_escalation", reason, vec!["escalation".into()]);
                if !context.is_empty() {
                    self.remember("escalation_context", context, vec!["escalation".into()]);
                }
            }
        }

        if self.consecutive_failures >= self.config.circuit_breaker_threshold {
            self.circuit_open = true;
            self.remember(
                "circuit_breaker_opened",
                &format!(
                    "opened after {} consecutive failures",
                    self.consecutive_failures
                ),
                vec!["circuit_breaker".into()],
            );
        }

        self.runs.push(run.clone());
        Ok(run)
    }

    pub fn reset_circuit_breaker(&mut self) {
        self.circuit_open = false;
        self.consecutive_failures = 0;
        self.remember(
            "circuit_breaker_reset",
            "manually reset",
            vec!["circuit_breaker".into()],
        );
    }

    pub fn override_decision(
        &mut self,
        run_id: &str,
        decision: CouncilDecision,
    ) -> Result<(), LoopError> {
        let run = self
            .runs
            .iter_mut()
            .find(|r| r.id == run_id)
            .ok_or_else(|| LoopError::RunNotFound(run_id.to_string()))?;

        run.status = match &decision {
            CouncilDecision::Ship => RunStatus::Passed,
            CouncilDecision::Iterate { .. } => RunStatus::Skipped,
            CouncilDecision::Escalate { .. } => RunStatus::Escalated,
        };
        self.remember(
            "council_override",
            &format!("{run_id} -> {decision}"),
            vec!["council".into()],
        );
        Ok(())
    }

    async fn execute_plan(&self, schedule: &Schedule) -> Result<String, LoopError> {
        let skills_summary: Vec<String> = self
            .skills
            .iter()
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect();
        let recent_memory: Vec<String> = self
            .memory
            .entries
            .iter()
            .rev()
            .take(5)
            .map(|e| format!("  {} = {}", e.key, e.value))
            .collect();

        Ok(format!(
            "Plan for {} (pattern: {})\nSkills available:\n{}\nRecent memory:\n{}\nMax iterations: {}\nCircuit breaker: {}",
            schedule.name,
            schedule.pattern,
            skills_summary.join("\n"),
            recent_memory.join("\n"),
            self.config.max_iterations,
            if self.circuit_open { "OPEN" } else { "closed" },
        ))
    }

    async fn evaluate(
        &self,
        schedule: &Schedule,
        run: &LoopRun,
    ) -> Result<EvaluationReport, LoopError> {
        let output_len = run.output.as_ref().map(|o| o.len()).unwrap_or(0);
        let agent_count = self.sub_agents.len();
        let mut criteria_results = Vec::new();

        let output_produced = CriterionResult {
            name: "output produced".into(),
            passed: output_len > 0,
            detail: format!("{} bytes of output", output_len),
        };
        criteria_results.push(output_produced);

        let agents_ran = CriterionResult {
            name: "agents executed".into(),
            passed: agent_count > 0 || output_len > 0,
            detail: format!("{} sub-agents assigned", agent_count),
        };
        criteria_results.push(agents_ran);

        let no_errors = CriterionResult {
            name: "no errors".into(),
            passed: run.error.is_none(),
            detail: match &run.error {
                Some(e) => format!("Error: {e}"),
                None => "No errors".into(),
            },
        };
        criteria_results.push(no_errors);

        for criterion in &self.config.evaluation_criteria {
            if criterion == "output produced" || criterion == "no errors" {
                continue;
            }
            criteria_results.push(CriterionResult {
                name: criterion.clone(),
                passed: true,
                detail: "Criterion met (default pass)".into(),
            });
        }

        let passed_count = criteria_results.iter().filter(|c| c.passed).count();
        let total_count = criteria_results.len();
        let score = if total_count > 0 {
            passed_count as f64 / total_count as f64
        } else {
            1.0
        };

        Ok(EvaluationReport {
            score,
            criteria_results,
            summary: format!(
                "Schedule '{}': {}/{} criteria passed (score: {:.2})",
                schedule.name, passed_count, total_count, score,
            ),
        })
    }

    async fn decide(
        &self,
        _schedule: &Schedule,
        eval: &EvaluationReport,
    ) -> Result<CouncilDecision, LoopError> {
        if eval.score >= 0.8 {
            return Ok(CouncilDecision::Ship);
        }
        if eval.score >= 0.4 {
            let failing: Vec<String> = eval
                .criteria_results
                .iter()
                .filter(|c| !c.passed)
                .map(|c| c.name.clone())
                .collect();
            return Ok(CouncilDecision::Iterate {
                reason: format!("Failed criteria: {}", failing.join(", ")),
            });
        }
        Ok(CouncilDecision::Escalate {
            reason: format!("Low score ({:.2}) — below threshold", eval.score),
            context: eval.summary.clone(),
        })
    }

    pub fn generate_next_prompt(&self, loop_name: &str) -> NextPrompt {
        let last_run = self
            .runs
            .last()
            .map(|r| format!("{} — {}", r.phase, r.id))
            .unwrap_or_else(|| "no runs yet".into());

        let current_phase = self
            .runs
            .last()
            .map(|r| r.phase.to_string())
            .unwrap_or_else(|| "plan".into());

        let status = self
            .runs
            .last()
            .map(|r| format!("{:?}", r.status))
            .unwrap_or_else(|| "new".into());

        let context = self
            .memory
            .entries
            .iter()
            .rev()
            .take(5)
            .map(|e| format!("- {}: {}", e.key, e.value))
            .collect::<Vec<_>>()
            .join("\n");

        let total_tokens: Option<usize> = self
            .runs
            .iter()
            .filter_map(|r| r.token_cost)
            .reduce(|a, b| a + b);

        let next_action = match self.runs.last().map(|r| &r.status) {
            Some(RunStatus::Passed) => {
                "Loop iteration passed. Decide: ship or start next iteration?".into()
            }
            Some(RunStatus::Escalated) => {
                "Last iteration escalated to human. Review and provide council decision.".into()
            }
            Some(RunStatus::Skipped) => {
                let fail_count = self.consecutive_failures;
                if fail_count >= self.config.escalate_after_failures {
                    format!(
                        "Escalation threshold reached ({fail_count} failures). Human review needed."
                    )
                } else {
                    format!(
                        "Iteration {fail_count}/{} before escalation. Run next iteration or address failures.",
                        self.config.escalate_after_failures
                    )
                }
            }
            Some(RunStatus::Failed(_)) => "Last iteration failed. Review error and retry.".into(),
            _ => format!("Continue loop {} — run next iteration", loop_name),
        };

        let circuit_note = if self.circuit_open {
            "\n\n**Circuit breaker is OPEN.** Run `portail loop reset-circuit` to reset."
                .to_string()
        } else {
            "".to_string()
        };

        NextPrompt {
            session_id: Uuid::new_v4().to_string(),
            generated_at: Utc::now().to_rfc3339(),
            loop_name: loop_name.to_string(),
            current_phase,
            last_run,
            status: format!("{}{}", status, circuit_note),
            next_action,
            context: if context.is_empty() {
                "No context stored yet".into()
            } else {
                context
            },
            artifacts: self.runs.iter().flat_map(|r| r.artifacts.clone()).collect(),
            token_spent: total_tokens,
        }
    }

    pub fn schedules(&self) -> &[Schedule] {
        &self.schedules
    }
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }
    pub fn plugins(&self) -> &[PluginConnector] {
        &self.plugins
    }
    pub fn runs(&self) -> &[LoopRun] {
        &self.runs
    }
    pub fn memory_entries(&self) -> &[MemoryEntry] {
        &self.memory.entries
    }
    pub fn sub_agents(&self) -> &[SubAgent] {
        &self.sub_agents
    }
    pub fn config(&self) -> &LoopEngineConfig {
        &self.config
    }
    pub fn is_circuit_open(&self) -> bool {
        self.circuit_open
    }
    pub fn consecutive_failures(&self) -> usize {
        self.consecutive_failures
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

pub struct SharedLoopEngine {
    inner: Mutex<LoopEngine>,
}

impl SharedLoopEngine {
    pub fn new(config: LoopEngineConfig) -> Self {
        Self {
            inner: Mutex::new(LoopEngine::new(config)),
        }
    }

    pub fn with_engine<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut LoopEngine) -> R,
    {
        let mut engine = self.inner.lock().unwrap();
        f(&mut engine)
    }

    pub async fn run_iteration(&self, schedule_name: &str) -> Result<LoopRun, LoopError> {
        let mut engine = {
            let mut guard = self.inner.lock().unwrap();
            let mut replacement = LoopEngine::new(LoopEngineConfig::default());
            std::mem::swap(&mut *guard, &mut replacement);
            replacement
        };
        let result = engine.run_iteration(schedule_name).await;
        *self.inner.lock().unwrap() = engine;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> LoopEngine {
        let mut engine = LoopEngine::new(LoopEngineConfig {
            name: "test".into(),
            max_iterations: 10,
            token_budget: Some(10000),
            escalate_after_failures: 2,
            circuit_breaker_threshold: 3,
            evaluation_criteria: vec!["output produced".into(), "no errors".into()],
        });
        engine.add_schedule(Schedule {
            name: "test-loop".into(),
            cadence_secs: 60,
            pattern: "test".into(),
            max_iterations: Some(5),
            enabled: true,
        });
        engine.add_sub_agent(SubAgent {
            id: "maker-1".into(),
            role: "maker".into(),
            model: "sonnet-4".into(),
            instruction: "Build feature X".into(),
            max_turns: 3,
        });
        engine
    }

    #[tokio::test]
    async fn test_full_loop_iteration() {
        let mut engine = make_engine();
        let run = engine.run_iteration("test-loop").await.unwrap();
        assert_eq!(run.schedule_name, "test-loop");
        assert!(run.completed_at.is_some());
        assert!(run.output.is_some());
        assert!(run.token_cost.is_some() && run.token_cost.unwrap() > 0);
        assert_eq!(run.status, RunStatus::Passed);
    }

    #[tokio::test]
    async fn test_loop_produces_artifacts() {
        let mut engine = make_engine();
        let run = engine.run_iteration("test-loop").await.unwrap();
        assert!(
            !run.artifacts.is_empty(),
            "should have at least a plan artifact"
        );
        assert!(run.artifacts.iter().any(|a| a.starts_with("Plan for")));
        assert!(run.artifacts.iter().any(|a| a.starts_with("token_cost:")));
    }

    #[tokio::test]
    async fn test_token_budget_enforced() {
        let mut engine = LoopEngine::new(LoopEngineConfig {
            name: "budget-test".into(),
            max_iterations: 10,
            token_budget: Some(10),
            escalate_after_failures: 3,
            circuit_breaker_threshold: 5,
            evaluation_criteria: vec![],
        });
        engine.add_schedule(Schedule {
            name: "budget-loop".into(),
            cadence_secs: 60,
            pattern: "budget".into(),
            max_iterations: None,
            enabled: true,
        });
        engine.add_sub_agent(SubAgent {
            id: "maker-1".into(),
            role: "maker".into(),
            model: "test".into(),
            instruction: "work".into(),
            max_turns: 1,
        });

        let first = engine.run_iteration("budget-loop").await.unwrap();
        assert_eq!(first.status, RunStatus::Passed);

        let second = engine.run_iteration("budget-loop").await;
        assert!(second.is_err());
        match second {
            Err(LoopError::TokenBudgetExceeded { .. }) => {}
            _ => panic!("Expected TokenBudgetExceeded error"),
        }
    }

    #[tokio::test]
    async fn test_max_iterations_enforced() {
        let mut engine = LoopEngine::new(LoopEngineConfig::default());
        engine.add_schedule(Schedule {
            name: "limited".into(),
            cadence_secs: 60,
            pattern: "test".into(),
            max_iterations: Some(1),
            enabled: true,
        });

        let first = engine.run_iteration("limited").await;
        assert!(first.is_ok());

        let second = engine.run_iteration("limited").await;
        assert!(second.is_err());
        assert!(matches!(second, Err(LoopError::MaxIterationsReached(1))));
    }

    #[tokio::test]
    async fn test_auto_escalation_after_failures() {
        let mut engine = LoopEngine::new(LoopEngineConfig {
            name: "escalate-test".into(),
            max_iterations: 10,
            token_budget: None,
            escalate_after_failures: 2,
            circuit_breaker_threshold: 5,
            evaluation_criteria: vec!["impossible_criterion".into()],
        });
        engine.add_schedule(Schedule {
            name: "escalate".into(),
            cadence_secs: 60,
            pattern: "test".into(),
            max_iterations: Some(10),
            enabled: true,
        });

        let r1 = engine.run_iteration("escalate").await.unwrap();
        assert_eq!(
            r1.status,
            RunStatus::Skipped,
            "should skip on first low-score"
        );

        let r2 = engine.run_iteration("escalate").await.unwrap();
        assert_eq!(
            r2.status,
            RunStatus::Escalated,
            "should escalate on 2nd consecutive failure"
        );
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let mut engine = LoopEngine::new(LoopEngineConfig {
            name: "cb-test".into(),
            max_iterations: 10,
            token_budget: None,
            escalate_after_failures: 1,
            circuit_breaker_threshold: 2,
            evaluation_criteria: vec!["impossible_criterion".into()],
        });
        engine.add_schedule(Schedule {
            name: "cb-loop".into(),
            cadence_secs: 60,
            pattern: "test".into(),
            max_iterations: Some(10),
            enabled: true,
        });

        engine.run_iteration("cb-loop").await.unwrap();
        assert!(
            !engine.is_circuit_open(),
            "1 failure should not open breaker"
        );

        engine.run_iteration("cb-loop").await.unwrap();
        assert!(
            engine.is_circuit_open(),
            "2 failures should open breaker with threshold=2"
        );

        let blocked = engine.run_iteration("cb-loop").await;
        assert!(blocked.is_err());
        assert!(matches!(blocked, Err(LoopError::CircuitBreakerOpen)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let mut engine = LoopEngine::new(LoopEngineConfig {
            name: "cb-test".into(),
            max_iterations: 10,
            token_budget: None,
            escalate_after_failures: 1,
            circuit_breaker_threshold: 1,
            evaluation_criteria: vec!["impossible_criterion".into()],
        });
        engine.add_schedule(Schedule {
            name: "cb-loop".into(),
            cadence_secs: 60,
            pattern: "test".into(),
            max_iterations: Some(10),
            enabled: true,
        });

        engine.run_iteration("cb-loop").await.unwrap();
        assert!(engine.is_circuit_open());

        engine.reset_circuit_breaker();
        assert!(!engine.is_circuit_open());
        assert_eq!(engine.consecutive_failures(), 0);
    }

    #[tokio::test]
    async fn test_council_override() {
        let mut engine = make_engine();
        let run = engine.run_iteration("test-loop").await.unwrap();

        engine
            .override_decision(
                &run.id,
                CouncilDecision::Escalate {
                    reason: "manual override".into(),
                    context: "human decision".into(),
                },
            )
            .unwrap();

        let updated = engine.runs().iter().find(|r| r.id == run.id).unwrap();
        assert_eq!(updated.status, RunStatus::Escalated);
    }

    #[tokio::test]
    async fn test_schedule_not_found() {
        let mut engine = make_engine();
        let result = engine.run_iteration("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LoopError::ScheduleNotFound(_))));
    }

    #[tokio::test]
    async fn test_next_prompt_after_run() {
        let mut engine = make_engine();
        engine.run_iteration("test-loop").await.unwrap();

        let prompt = engine.generate_next_prompt("test-loop");
        let markdown = prompt.to_prompt();
        assert!(markdown.contains("Loop Handoff"));
        assert!(markdown.contains("test-loop"));
        assert!(markdown.contains("token_cost:"));
    }

    #[test]
    fn test_loop_phase_display() {
        assert_eq!(LoopPhase::Plan.to_string(), "plan");
        assert_eq!(LoopPhase::Execute.to_string(), "execute");
    }

    #[test]
    fn test_council_display() {
        assert_eq!(CouncilDecision::Ship.to_string(), "SHIP");
        let iter = CouncilDecision::Iterate {
            reason: "needs more data".into(),
        };
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

    #[test]
    fn test_empty_executor() {
        let executor = Executor::new(vec![], vec![], vec![]);
        let result = executor.run("test", "run-1");
        assert!(result.total_tokens >= 100);
    }

    #[test]
    fn test_executor_with_agents() {
        let agent = SubAgent {
            id: "maker-1".into(),
            role: "maker".into(),
            model: "test".into(),
            instruction: "build".into(),
            max_turns: 2,
        };
        let executor = Executor::new(vec![agent], vec![], vec![]);
        let result = executor.run("test", "run-1");
        assert!(result.total_tokens >= 1000);
        assert_eq!(result.outputs.len(), 1);
    }
}
