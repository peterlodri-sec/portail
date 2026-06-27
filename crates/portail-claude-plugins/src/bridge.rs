//! Skill → MCP tool bridge.
//!
//! Converts parsed `SkillDefinition`s and `CommandDefinition`s into
//! callable MCP tool definitions. Any Portail agent can invoke these
//! tools via the standard MCP `tools/call` interface.
//!
//! # Tool Naming
//!
//! Skills become: `skill:<plugin>:<slug>`
//! Commands become: `cmd:<plugin>:<slug>`
//!
//! # Execution Model
//!
//! - Commands with `allowed-tools` containing `Bash(...)` → subprocess exec
//! - Commands with only non-Bash tools → return skill body as context
//! - Skills (no tool perms) → return body as LLM context injection

use crate::plugin::LoadedPlugin;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// An MCP tool definition ready for registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (e.g. "skill:tavily-search:search").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for input parameters.
    pub input_schema: serde_json::Value,
    /// Source plugin name.
    pub plugin: String,
    /// Source slug (skill or command name).
    pub slug: String,
    /// Whether this tool can execute commands.
    pub executable: bool,
}

/// Result of executing a tool call.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Text output from the tool.
    pub output: String,
    /// Whether the execution failed.
    pub is_error: bool,
    /// Execution latency.
    pub latency: Duration,
}

/// Registry of all tools derived from loaded plugins.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
    /// Skill bodies indexed by tool name (for context injection).
    skill_bodies: HashMap<String, String>,
    /// Command bodies indexed by tool name.
    command_bodies: HashMap<String, String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register all skills and commands from a loaded plugin.
    pub fn register_plugin(&mut self, plugin: &LoadedPlugin) {
        let plugin_name = plugin.name().to_string();

        for skill in &plugin.skills {
            let slug = skill.name.clone().unwrap_or_default();
            let tool_name = format!("skill:{plugin_name}:{slug}");

            let description = skill
                .description
                .clone()
                .unwrap_or_else(|| format!("Skill from {plugin_name}"));

            let input_schema = if skill.body.is_empty() {
                serde_json::json!({ "type": "object", "properties": {} })
            } else {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Optional query or context to refine the skill"
                        }
                    }
                })
            };

            self.skill_bodies
                .insert(tool_name.clone(), skill.body.clone());

            self.tools.insert(
                tool_name.clone(),
                ToolDefinition {
                    name: tool_name,
                    description,
                    input_schema,
                    plugin: plugin_name.clone(),
                    slug,
                    executable: false,
                },
            );
        }

        for cmd in &plugin.commands {
            let slug = cmd.name.clone().unwrap_or_default();
            let tool_name = format!("cmd:{plugin_name}:{slug}");

            let description = cmd
                .description
                .clone()
                .unwrap_or_else(|| format!("Command from {plugin_name}"));

            let mut properties = serde_json::Map::new();
            properties.insert(
                "query".to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": "Input query or arguments"
                }),
            );

            // Add argument hint as a parameter if present
            if let Some(hint) = &cmd.argument_hint {
                properties.insert(
                    "arguments".to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": format!("Arguments: {hint}")
                    }),
                );
            }

            let input_schema = serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": ["query"]
            });

            let has_bash = cmd.allowed_tools.iter().any(|t| t.starts_with("Bash"));

            self.command_bodies
                .insert(tool_name.clone(), cmd.body.clone());

            self.tools.insert(
                tool_name.clone(),
                ToolDefinition {
                    name: tool_name,
                    description,
                    input_schema,
                    plugin: plugin_name.clone(),
                    slug,
                    executable: has_bash,
                },
            );
        }
    }

    /// Register all plugins at once.
    pub fn register_plugins(&mut self, plugins: &[LoadedPlugin]) {
        for plugin in plugins {
            self.register_plugin(plugin);
        }
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// Get a tool by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Get the body for a skill tool.
    pub fn get_skill_body(&self, tool_name: &str) -> Option<&str> {
        self.skill_bodies.get(tool_name).map(|s| s.as_str())
    }

    /// Get the body for a command tool.
    pub fn get_command_body(&self, tool_name: &str) -> Option<&str> {
        self.command_bodies.get(tool_name).map(|s| s.as_str())
    }

    /// Total number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Execute a tool call.
///
/// For now, this returns the skill/command body as context.
/// A full implementation would dispatch to the appropriate executor
/// (subprocess for Bash tools, LLM for prompt tools, etc.).
pub fn execute_tool(
    registry: &ToolRegistry,
    tool_name: &str,
    input: &serde_json::Value,
) -> ToolResult {
    let start = std::time::Instant::now();

    let tool = match registry.get_tool(tool_name) {
        Some(t) => t,
        None => {
            return ToolResult {
                output: format!("tool not found: {tool_name}"),
                is_error: true,
                latency: start.elapsed(),
            };
        }
    };

    // Try command body first (commands have tool permissions)
    if let Some(body) = registry.get_command_body(tool_name) {
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let args = input
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let output = if body.is_empty() {
            format!("Command '{query}' — no instructions defined")
        } else {
            format!(
                "# {}\n\n{body}\n\n---\n**Input:** {query} {args}",
                tool.slug
            )
        };

        return ToolResult {
            output,
            is_error: false,
            latency: start.elapsed(),
        };
    }

    // Fall back to skill body (instructions only)
    if let Some(body) = registry.get_skill_body(tool_name) {
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let output = if body.is_empty() {
            format!("Skill '{query}' — no instructions defined")
        } else {
            format!("# {}\n\n{body}\n\n---\n**Context:** {query}", tool.slug)
        };

        return ToolResult {
            output,
            is_error: false,
            latency: start.elapsed(),
        };
    }

    ToolResult {
        output: format!("no body registered for tool: {tool_name}"),
        is_error: true,
        latency: start.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::skill::{CommandDefinition, SkillDefinition};
    use std::path::PathBuf;

    fn test_plugin() -> LoadedPlugin {
        LoadedPlugin {
            root: PathBuf::from("/tmp/test"),
            manifest: Some(PluginManifest {
                name: "test-plugin".into(),
                version: Some("1.0.0".into()),
                description: Some("test".into()),
                author: None,
                homepage: None,
                repository: None,
                license: None,
                keywords: vec![],
                commands: None,
                agents: None,
                skills: None,
                hooks: None,
                mcp_servers: None,
                lsp_servers: None,
                output_styles: None,
            }),
            skills: vec![SkillDefinition {
                name: Some("search".into()),
                description: Some("Search the web".into()),
                disable_model_invocation: false,
                body: "Use `tvly search` to search.".into(),
                path: "skills/search/SKILL.md".into(),
                plugin: None,
            }],
            commands: vec![CommandDefinition {
                name: Some("deploy".into()),
                description: Some("Deploy to production".into()),
                argument_hint: Some("<env>".into()),
                allowed_tools: vec!["Bash(deploy *)".into()],
                agent: None,
                body: "Run deploy script.".into(),
                path: "commands/deploy.md".into(),
                plugin: None,
            }],
            agents: vec![],
            hooks: None,
            mcp_servers: None,
            lsp_servers: None,
        }
    }

    #[test]
    fn register_and_list() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(&test_plugin());
        assert_eq!(reg.len(), 2);
        assert!(reg.get_tool("skill:test-plugin:search").is_some());
        assert!(reg.get_tool("cmd:test-plugin:deploy").is_some());
    }

    #[test]
    fn tool_namespacing() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(&test_plugin());
        let tools = reg.list_tools();
        assert!(tools.iter().all(|t| t.plugin == "test-plugin"));
    }

    #[test]
    fn execute_skill_tool() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(&test_plugin());

        let result = execute_tool(
            &reg,
            "skill:test-plugin:search",
            &serde_json::json!({"query": "rust async"}),
        );
        assert!(!result.is_error);
        assert!(result.output.contains("tvly search"));
        assert!(result.output.contains("rust async"));
    }

    #[test]
    fn execute_command_tool() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(&test_plugin());

        let result = execute_tool(
            &reg,
            "cmd:test-plugin:deploy",
            &serde_json::json!({"query": "staging", "arguments": "--force"}),
        );
        assert!(!result.is_error);
        assert!(result.output.contains("deploy"));
        assert!(result.output.contains("staging"));
    }

    #[test]
    fn execute_unknown_tool() {
        let reg = ToolRegistry::new();
        let result = execute_tool(&reg, "unknown", &serde_json::json!({}));
        assert!(result.is_error);
        assert!(result.output.contains("not found"));
    }

    #[test]
    fn command_executable_flag() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(&test_plugin());
        let cmd = reg.get_tool("cmd:test-plugin:deploy").unwrap();
        assert!(cmd.executable);

        let skill = reg.get_tool("skill:test-plugin:search").unwrap();
        assert!(!skill.executable);
    }
}
