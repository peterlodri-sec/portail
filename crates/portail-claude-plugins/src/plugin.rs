//! Plugin loader — scans a directory and loads all components.
//!
//! Mirrors the Python SDK's `load_plugin()`:
//! - `.claude-plugin/plugin.json` → manifest
//! - `skills/*/SKILL.md` → skills
//! - `commands/*.md` → commands
//! - `agents/*.md` → agents
//! - `hooks/hooks.json` → hooks config
//! - `.mcp.json` → MCP servers
//! - `.lsp.json` → LSP servers

use crate::manifest::{HooksConfig, LspServersConfig, McpServersConfig, PluginManifest};
use crate::skill::{AgentDefinition, CommandDefinition, SkillDefinition};
use crate::skill::{parse_agent_file, parse_command_file, parse_skill_file};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// A fully loaded Claude plugin.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub root: PathBuf,
    pub manifest: Option<PluginManifest>,
    pub skills: Vec<SkillDefinition>,
    pub commands: Vec<CommandDefinition>,
    pub agents: Vec<AgentDefinition>,
    pub hooks: Option<HooksConfig>,
    pub mcp_servers: Option<McpServersConfig>,
    pub lsp_servers: Option<LspServersConfig>,
}

impl LoadedPlugin {
    /// Plugin name from manifest, or directory name.
    pub fn name(&self) -> &str {
        self.manifest
            .as_ref()
            .map(|m| m.name.as_str())
            .unwrap_or_else(|| {
                self.root
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
            })
    }

    /// All skill names namespaced as `plugin:skill`.
    pub fn skill_ids(&self) -> Vec<String> {
        let plugin = self.name();
        self.skills
            .iter()
            .map(|s| {
                let slug = s.name.as_deref().unwrap_or("unnamed");
                format!("{plugin}:{slug}")
            })
            .collect()
    }

    /// All command names namespaced as `plugin:command`.
    pub fn command_ids(&self) -> Vec<String> {
        let plugin = self.name();
        self.commands
            .iter()
            .map(|c| {
                let slug = c.name.as_deref().unwrap_or("unnamed");
                format!("{plugin}:{slug}")
            })
            .collect()
    }

    /// All agent names namespaced as `plugin:agent`.
    pub fn agent_ids(&self) -> Vec<String> {
        let plugin = self.name();
        self.agents
            .iter()
            .map(|a| format!("{plugin}:{}", a.name))
            .collect()
    }
}

/// Load a plugin from a directory.
///
/// The directory must contain either a `.claude-plugin/plugin.json`
/// manifest or at least one component (skills/, commands/, agents/).
pub fn load_plugin(root: &Path) -> Result<LoadedPlugin> {
    if !root.is_dir() {
        anyhow::bail!("not a directory: {}", root.display());
    }

    // 1. Manifest (optional)
    let manifest = load_manifest(root)?;

    // 2. Skills: skills/*/SKILL.md
    let skills = load_skills(root)?;

    // 3. Commands: commands/*.md
    let commands = load_commands(root)?;

    // 4. Agents: agents/*.md
    let agents = load_agents(root)?;

    // 5. Hooks: hooks/hooks.json
    let hooks = load_hooks(root)?;

    // 6. MCP: .mcp.json
    let mcp_servers = load_mcp(root)?;

    // 7. LSP: .lsp.json
    let lsp_servers = load_lsp(root)?;

    let plugin = LoadedPlugin {
        root: root.to_path_buf(),
        manifest,
        skills,
        commands,
        agents,
        hooks,
        mcp_servers,
        lsp_servers,
    };

    tracing::info!(
        plugin = plugin.name(),
        skills = plugin.skills.len(),
        commands = plugin.commands.len(),
        agents = plugin.agents.len(),
        "loaded Claude plugin"
    );

    Ok(plugin)
}

fn load_manifest(root: &Path) -> Result<Option<PluginManifest>> {
    let path = root.join(".claude-plugin").join("plugin.json");
    if !path.exists() {
        return Ok(None);
    }
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: PluginManifest =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(manifest))
}

fn load_skills(root: &Path) -> Result<Vec<SkillDefinition>> {
    let skills_dir = root.join("skills");
    let mut skills = Vec::new();

    if !skills_dir.is_dir() {
        return Ok(skills);
    }

    for entry in std::fs::read_dir(&skills_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let skill_md = entry.path().join("SKILL.md");
        if skill_md.exists() {
            match parse_skill_file(&skill_md) {
                Ok(mut skill) => {
                    skill.plugin = None; // set by caller
                    skills.push(skill);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %skill_md.display(),
                        error = %e,
                        "failed to parse SKILL.md"
                    );
                }
            }
        }
    }

    Ok(skills)
}

fn load_commands(root: &Path) -> Result<Vec<CommandDefinition>> {
    let commands_dir = root.join("commands");
    let mut commands = Vec::new();

    if !commands_dir.is_dir() {
        return Ok(commands);
    }

    for entry in std::fs::read_dir(&commands_dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
            match parse_command_file(&entry.path()) {
                Ok(mut cmd) => {
                    cmd.plugin = None;
                    commands.push(cmd);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %entry.path().display(),
                        error = %e,
                        "failed to parse command"
                    );
                }
            }
        }
    }

    Ok(commands)
}

fn load_agents(root: &Path) -> Result<Vec<AgentDefinition>> {
    let agents_dir = root.join("agents");
    let mut agents = Vec::new();

    if !agents_dir.is_dir() {
        return Ok(agents);
    }

    for entry in std::fs::read_dir(&agents_dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
            match parse_agent_file(&entry.path()) {
                Ok(mut agent) => {
                    agent.plugin = None;
                    agents.push(agent);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %entry.path().display(),
                        error = %e,
                        "failed to parse agent"
                    );
                }
            }
        }
    }

    Ok(agents)
}

fn load_hooks(root: &Path) -> Result<Option<HooksConfig>> {
    let path = root.join("hooks").join("hooks.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let config: HooksConfig = serde_json::from_str(&content)?;
    Ok(Some(config))
}

fn load_mcp(root: &Path) -> Result<Option<McpServersConfig>> {
    let path = root.join(".mcp.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let config: McpServersConfig = serde_json::from_str(&content)?;
    Ok(Some(config))
}

fn load_lsp(root: &Path) -> Result<Option<LspServersConfig>> {
    let path = root.join(".lsp.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let config: LspServersConfig = serde_json::from_str(&content)?;
    Ok(Some(config))
}

/// Scan multiple directories for plugins.
pub fn scan_plugin_dirs(dirs: &[&Path]) -> Vec<LoadedPlugin> {
    let mut plugins = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(dir).into_iter().flatten() {
            let entry = entry.unwrap();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                match load_plugin(&entry.path()) {
                    Ok(plugin) => plugins.push(plugin),
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path().display(),
                            error = %e,
                            "failed to load plugin"
                        );
                    }
                }
            }
        }
    }
    plugins
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_plugin(dir: &Path) {
        // manifest
        let manifest_dir = dir.join(".claude-plugin");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::write(
            manifest_dir.join("plugin.json"),
            r#"{"name": "test-plugin", "version": "1.0.0", "description": "A test"}"#,
        )
        .unwrap();

        // skill
        let skill_dir = dir.join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: my-skill
description: Does something useful
---

Do the thing.
"#,
        )
        .unwrap();

        // command
        let commands_dir = dir.join("commands");
        fs::create_dir_all(&commands_dir).unwrap();
        fs::write(
            commands_dir.join("search.md"),
            r#"---
name: search
description: Search the web
allowed-tools:
  - Bash(tvly *)
---

Search instructions.
"#,
        )
        .unwrap();

        // agent
        let agents_dir = dir.join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(
            agents_dir.join("reviewer.md"),
            r#"---
name: reviewer
description: Code reviewer
tools: Read, Grep
---

Review code.
"#,
        )
        .unwrap();

        // hooks
        let hooks_dir = dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        fs::write(
            hooks_dir.join("hooks.json"),
            r#"{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo hook"}]}]}}"#,
        )
        .unwrap();

        // mcp
        fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers": {"my-server": {"command": "node", "args": ["server.js"]}}}"#,
        )
        .unwrap();
    }

    #[test]
    fn load_full_plugin() {
        let tmp = std::env::temp_dir().join("portail-test-plugin-full");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        create_test_plugin(&tmp);

        let plugin = load_plugin(&tmp).unwrap();
        assert_eq!(plugin.name(), "test-plugin");
        assert!(plugin.manifest.is_some());
        assert_eq!(plugin.skills.len(), 1);
        assert_eq!(plugin.commands.len(), 1);
        assert_eq!(plugin.agents.len(), 1);
        assert!(plugin.hooks.is_some());
        assert!(plugin.mcp_servers.is_some());

        assert_eq!(plugin.skill_ids(), vec!["test-plugin:my-skill"]);
        assert_eq!(plugin.command_ids(), vec!["test-plugin:search"]);
        assert_eq!(plugin.agent_ids(), vec!["test-plugin:reviewer"]);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_minimal_plugin() {
        let tmp = std::env::temp_dir().join("portail-test-plugin-minimal");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Just a skill, no manifest
        let skill_dir = tmp.join("skills").join("basic");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Basic\n\nDo stuff.").unwrap();

        let plugin = load_plugin(&tmp).unwrap();
        assert_eq!(plugin.name(), "portail-test-plugin-minimal"); // fallback to dir name
        assert!(plugin.manifest.is_none());
        assert_eq!(plugin.skills.len(), 1);

        let _ = fs::remove_dir_all(&tmp);
    }
}
