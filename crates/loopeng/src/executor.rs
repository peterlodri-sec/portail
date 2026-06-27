use crate::types::{PluginConnector, Skill, SubAgent};

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
        Self { sub_agents, skills, plugins }
    }

    pub fn run(&self, _schedule_name: &str, _run_id: &str) -> super::types::ExecutionResult {
        let mut agent_outputs = Vec::new();
        let mut total_tokens: usize = 0;

        for agent in &self.sub_agents {
            let turns = agent.max_turns.min(5);
            let tokens_per_turn = 500;
            let agent_cost = turns * tokens_per_turn;
            total_tokens += agent_cost;

            agent_outputs.push(format!(
                "[{}] {} — {} ({} turns, ~{} tokens): {}",
                agent.role, agent.id, agent.instruction, turns, agent_cost,
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

        super::types::ExecutionResult {
            outputs: agent_outputs,
            total_tokens: if self.sub_agents.is_empty() && self.skills.is_empty() {
                100
            } else {
                total_tokens
            },
        }
    }
}
