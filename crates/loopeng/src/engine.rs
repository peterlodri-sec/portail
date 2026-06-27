use chrono::Utc;
use std::sync::Mutex;
use uuid::Uuid;

use crate::executor::Executor;
use crate::types::*;

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
            memory: LoopMemory { entries: Vec::new(), max_entries: 100 },
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
        self.memory.entries.iter().rev().filter(|e| e.tags.contains(&tag.to_string())).collect()
    }

    pub async fn run_iteration(&mut self, schedule_name: &str) -> Result<LoopRun, LoopError> {
        if self.circuit_open {
            return Err(LoopError::CircuitBreakerOpen);
        }

        let schedule = self.schedules.iter().find(|s| s.name == schedule_name)
            .ok_or_else(|| LoopError::ScheduleNotFound(schedule_name.to_string()))?;

        if let Some(max_it) = schedule.max_iterations {
            let count = self.runs.iter().filter(|r| r.schedule_name == schedule_name).count();
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
        run.artifacts.push(format!("token_cost: {}", exec_result.total_tokens));

        run.phase = LoopPhase::Evaluate;
        let eval_report = self.evaluate(schedule, &run).await?;
        run.artifacts.push(format!("eval_score: {:.2}", eval_report.score));

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
                &format!("opened after {} consecutive failures", self.consecutive_failures),
                vec!["circuit_breaker".into()],
            );
        }

        self.runs.push(run.clone());
        Ok(run)
    }

    pub fn reset_circuit_breaker(&mut self) {
        self.circuit_open = false;
        self.consecutive_failures = 0;
        self.remember("circuit_breaker_reset", "manually reset", vec!["circuit_breaker".into()]);
    }

    pub fn override_decision(&mut self, run_id: &str, decision: CouncilDecision) -> Result<(), LoopError> {
        let run = self.runs.iter_mut().find(|r| r.id == run_id)
            .ok_or_else(|| LoopError::RunNotFound(run_id.to_string()))?;

        run.status = match &decision {
            CouncilDecision::Ship => RunStatus::Passed,
            CouncilDecision::Iterate { .. } => RunStatus::Skipped,
            CouncilDecision::Escalate { .. } => RunStatus::Escalated,
        };
        self.remember("council_override", &format!("{run_id} -> {decision}"), vec!["council".into()]);
        Ok(())
    }

    async fn execute_plan(&self, schedule: &Schedule) -> Result<String, LoopError> {
        let skills_summary: Vec<String> = self.skills.iter()
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect();
        let recent_memory: Vec<String> = self.memory.entries.iter().rev().take(5)
            .map(|e| format!("  {} = {}", e.key, e.value))
            .collect();

        Ok(format!(
            "Plan for {} (pattern: {})\nSkills available:\n{}\nRecent memory:\n{}\nMax iterations: {}\nCircuit breaker: {}",
            schedule.name, schedule.pattern,
            skills_summary.join("\n"),
            recent_memory.join("\n"),
            self.config.max_iterations,
            if self.circuit_open { "OPEN" } else { "closed" },
        ))
    }

    async fn evaluate(&self, schedule: &Schedule, run: &LoopRun) -> Result<EvaluationReport, LoopError> {
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

    async fn decide(&self, _schedule: &Schedule, eval: &EvaluationReport) -> Result<CouncilDecision, LoopError> {
        if eval.score >= 0.8 {
            return Ok(CouncilDecision::Ship);
        }
        if eval.score >= 0.4 {
            let failing: Vec<String> = eval.criteria_results.iter()
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
        let last_run = self.runs.last()
            .map(|r| format!("{} — {}", r.phase, r.id))
            .unwrap_or_else(|| "no runs yet".into());

        let current_phase = self.runs.last()
            .map(|r| r.phase.to_string())
            .unwrap_or_else(|| "plan".into());

        let status = self.runs.last()
            .map(|r| format!("{:?}", r.status))
            .unwrap_or_else(|| "new".into());

        let context = self.memory.entries.iter().rev().take(5)
            .map(|e| format!("- {}: {}", e.key, e.value))
            .collect::<Vec<_>>()
            .join("\n");

        let total_tokens: Option<usize> = self.runs.iter()
            .filter_map(|r| r.token_cost)
            .reduce(|a, b| a + b);

        let next_action = match self.runs.last().map(|r| &r.status) {
            Some(RunStatus::Passed) => "Loop iteration passed. Decide: ship or start next iteration?".into(),
            Some(RunStatus::Escalated) => "Last iteration escalated to human. Review and provide council decision.".into(),
            Some(RunStatus::Skipped) => {
                let fail_count = self.consecutive_failures;
                if fail_count >= self.config.escalate_after_failures {
                    format!("Escalation threshold reached ({fail_count} failures). Human review needed.")
                } else {
                    format!("Iteration {fail_count}/{} before escalation. Run next iteration or address failures.", self.config.escalate_after_failures)
                }
            }
            Some(RunStatus::Failed(_)) => "Last iteration failed. Review error and retry.".into(),
            _ => format!("Continue loop {} — run next iteration", loop_name),
        };

        let circuit_note = if self.circuit_open {
            "\n\n**Circuit breaker is OPEN.** Run `portail loop reset-circuit` to reset.".to_string()
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
            context: if context.is_empty() { "No context stored yet".into() } else { context },
            artifacts: self.runs.iter().flat_map(|r| r.artifacts.clone()).collect(),
            token_spent: total_tokens,
        }
    }

    pub fn schedules(&self) -> &[Schedule] { &self.schedules }
    pub fn skills(&self) -> &[Skill] { &self.skills }
    pub fn plugins(&self) -> &[PluginConnector] { &self.plugins }
    pub fn runs(&self) -> &[LoopRun] { &self.runs }
    pub fn memory_entries(&self) -> &[MemoryEntry] { &self.memory.entries }
    pub fn sub_agents(&self) -> &[SubAgent] { &self.sub_agents }
    pub fn config(&self) -> &LoopEngineConfig { &self.config }
    pub fn is_circuit_open(&self) -> bool { self.circuit_open }
    pub fn consecutive_failures(&self) -> usize { self.consecutive_failures }
}

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
        assert!(!run.artifacts.is_empty(), "should have at least a plan artifact");
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
        assert_eq!(r1.status, RunStatus::Skipped, "should skip on first low-score");

        let r2 = engine.run_iteration("escalate").await.unwrap();
        assert_eq!(r2.status, RunStatus::Escalated, "should escalate on 2nd consecutive failure");
    }
}
