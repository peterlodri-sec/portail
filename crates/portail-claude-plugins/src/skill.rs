//! SKILL.md, command, and agent parsers.
//!
//! Mirrors the Python SDK's `load_skill()`, `load_command()`, `load_agent()`.
//! All three use the same pattern: YAML frontmatter + markdown body.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Skill (skills/*/SKILL.md) ───────────────────────────────────

/// A skill is pure instructions — no tool permissions.
/// Tool permissions live on `CommandDefinition`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillDefinition {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, alias = "disable-model-invocation")]
    pub disable_model_invocation: bool,
    #[serde(default)]
    pub body: String,
    /// Source file path (set during loading, not from frontmatter).
    #[serde(skip)]
    pub path: String,
    /// Parent plugin name (set during loading).
    #[serde(skip)]
    pub plugin: Option<String>,
}

// ─── Command (commands/*.md) ─────────────────────────────────────

/// A command is invocable and has tool permissions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandDefinition {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, alias = "argument-hint")]
    pub argument_hint: Option<String>,
    #[serde(default, alias = "allowed-tools")]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(skip)]
    pub path: String,
    #[serde(skip)]
    pub plugin: Option<String>,
}

// ─── Agent (agents/*.md) ─────────────────────────────────────────

/// A subagent definition.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, deserialize_with = "deserialize_tools")]
    pub tools: Vec<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(skip)]
    pub path: String,
    #[serde(skip)]
    pub plugin: Option<String>,
}

/// Deserialize tools from either a comma-separated string or a list.
fn deserialize_tools<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ToolsInput {
        List(Vec<String>),
        CommaSeparated(String),
    }

    let input = ToolsInput::deserialize(deserializer)?;
    match input {
        ToolsInput::List(v) => Ok(v),
        ToolsInput::CommaSeparated(s) => Ok(s
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()),
    }
}

// ─── Shared frontmatter parser ───────────────────────────────────

/// Split content into (frontmatter_yaml, body).
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return (None, content);
    }
    let after_open = &content[3..];
    let after_open = after_open.trim_start_matches('\n');
    if let Some(end) = after_open.find("\n---") {
        let fm = &after_open[..end];
        let body = &after_open[end + 4..];
        let body = body.trim_start_matches('\n');
        (Some(fm), body)
    } else {
        (None, content)
    }
}

/// Parse a SKILL.md file.
pub fn parse_skill_file(path: &Path) -> anyhow::Result<SkillDefinition> {
    let content = std::fs::read_to_string(path)?;
    let (fm_yaml, body) = split_frontmatter(&content);

    let mut def: SkillDefinition = if let Some(yaml) = fm_yaml {
        serde_yml::from_str(yaml)?
    } else {
        SkillDefinition::default()
    };

    def.body = body.to_string();
    def.path = path.to_string_lossy().to_string();

    // Fallback name: directory name
    if def.name.is_none() {
        def.name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
    }

    Ok(def)
}

/// Parse a commands/*.md file.
pub fn parse_command_file(path: &Path) -> anyhow::Result<CommandDefinition> {
    let content = std::fs::read_to_string(path)?;
    let (fm_yaml, body) = split_frontmatter(&content);

    let mut def: CommandDefinition = if let Some(yaml) = fm_yaml {
        serde_yml::from_str(yaml)?
    } else {
        CommandDefinition::default()
    };

    def.body = body.to_string();
    def.path = path.to_string_lossy().to_string();

    // Fallback name: filename without extension
    if def.name.is_none() {
        def.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
    }

    Ok(def)
}

/// Parse an agents/*.md file.
pub fn parse_agent_file(path: &Path) -> anyhow::Result<AgentDefinition> {
    let content = std::fs::read_to_string(path)?;
    let (fm_yaml, body) = split_frontmatter(&content);

    let mut def: AgentDefinition = if let Some(yaml) = fm_yaml {
        serde_yml::from_str(yaml)?
    } else {
        AgentDefinition::default()
    };

    def.body = body.to_string();
    def.path = path.to_string_lossy().to_string();

    // Fallback name: filename without extension
    if def.name.is_empty() {
        def.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
    }

    Ok(def)
}

// ─── tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_basic() {
        let md = r#"---
name: tavily-search
description: Search the web with LLM-optimized results
---

# Tavily Search

Use `tvly search "query"` to search.
"#;
        let skill = parse_skill_content(md);
        assert_eq!(skill.name.as_deref(), Some("tavily-search"));
        assert_eq!(
            skill.description.as_deref(),
            Some("Search the web with LLM-optimized results")
        );
        assert!(skill.body.contains("# Tavily Search"));
        assert!(!skill.disable_model_invocation);
    }

    #[test]
    fn parse_skill_no_frontmatter() {
        let md = "# My Skill\n\nJust instructions.";
        let skill = parse_skill_content(md);
        assert!(skill.name.is_none());
        assert!(skill.description.is_none());
        assert_eq!(skill.body, "# My Skill\n\nJust instructions.");
    }

    #[test]
    fn parse_skill_disable_invocation() {
        let md = r#"---
name: test
disable-model-invocation: true
---

Body.
"#;
        let skill = parse_skill_content(md);
        assert!(skill.disable_model_invocation);
    }

    #[test]
    fn parse_command_with_allowed_tools() {
        let md = r#"---
name: search
description: Search the web
argument-hint: <query>
allowed-tools:
  - Bash(tvly *)
  - WebFetch(domain:example.com)
---

Search instructions.
"#;
        let cmd = parse_command_content(md);
        assert_eq!(cmd.name.as_deref(), Some("search"));
        assert_eq!(cmd.argument_hint.as_deref(), Some("<query>"));
        assert_eq!(cmd.allowed_tools.len(), 2);
        assert_eq!(cmd.allowed_tools[0], "Bash(tvly *)");
    }

    #[test]
    fn parse_agent_with_tools() {
        let md = r#"---
name: reviewer
description: Code reviewer agent
tools: Read, Grep, Glob
color: blue
---

Review code carefully.
"#;
        let agent = parse_agent_content(md);
        assert_eq!(agent.name, "reviewer");
        assert_eq!(agent.tools.len(), 3);
        assert_eq!(agent.color.as_deref(), Some("blue"));
    }

    #[test]
    fn parse_agent_tools_as_list() {
        let md = r#"---
name: reviewer
description: Code reviewer
tools:
  - Read
  - Grep
---

Body.
"#;
        let agent = parse_agent_content(md);
        assert_eq!(agent.tools, vec!["Read", "Grep"]);
    }

    fn parse_skill_content(content: &str) -> SkillDefinition {
        let (fm, body) = split_frontmatter(content);
        let mut def: SkillDefinition = fm
            .map(|y| serde_yml::from_str(y).unwrap())
            .unwrap_or_default();
        def.body = body.to_string();
        def
    }

    fn parse_command_content(content: &str) -> CommandDefinition {
        let (fm, body) = split_frontmatter(content);
        let mut def: CommandDefinition = fm
            .map(|y| serde_yml::from_str(y).unwrap())
            .unwrap_or_default();
        def.body = body.to_string();
        def
    }

    fn parse_agent_content(content: &str) -> AgentDefinition {
        let (fm, body) = split_frontmatter(content);
        let mut def: AgentDefinition = fm
            .map(|y| serde_yml::from_str(y).unwrap())
            .unwrap_or_default();
        def.body = body.to_string();
        def
    }

    #[test]
    fn parse_real_skills_from_disk() {
        let skills_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default() + "/.claude/skills");
        if !skills_dir.is_dir() {
            eprintln!("skipping: {} not found", skills_dir.display());
            return;
        }

        let mut parsed = 0;
        let mut failed = 0;

        for entry in std::fs::read_dir(&skills_dir).into_iter().flatten() {
            let entry = entry.unwrap();
            if !entry.file_type().unwrap().is_dir() {
                continue;
            }
            let skill_md = entry.path().join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            match parse_skill_file(&skill_md) {
                Ok(_skill) => {
                    // Body can be empty (placeholder skills exist)
                    parsed += 1;
                }
                Err(e) => {
                    eprintln!("FAILED to parse {}: {}", skill_md.display(), e);
                    failed += 1;
                }
            }
        }

        eprintln!("parsed {parsed} real SKILL.md files, {failed} failed");
        assert!(parsed > 0, "should parse at least one real SKILL.md");
        assert_eq!(failed, 0, "no real SKILL.md files should fail to parse");
    }
}
