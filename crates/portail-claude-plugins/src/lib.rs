//! Claude Code plugin system for Portail.
//!
//! Mirrors the Python `claude-code-plugins-sdk` types and loading logic
//! in pure Rust. Parses `.claude-plugin/plugin.json`, `SKILL.md`,
//! `hooks.json`, `.mcp.json`, and `marketplace.json`.
//!
//! # Plugin Directory Layout
//!
//! ```text
//! plugin-name/
//! ├── .claude-plugin/
//! │   └── plugin.json          # Manifest
//! ├── hooks/
//! │   └── hooks.json           # Hook definitions
//! ├── skills/
//! │   └── skill-name/
//! │       └── SKILL.md         # Skill (instructions only)
//! ├── commands/
//! │   └── command-name.md      # Command (invocable, has tool perms)
//! ├── agents/
//! │   └── agent-name.md        # Subagent definition
//! ├── .mcp.json                # MCP server configs
//! ├── .lsp.json                # LSP server configs
//! └── README.md
//! ```

pub mod bridge;
pub mod manifest;
pub mod marketplace;
pub mod plugin;
pub mod skill;
pub mod trust;

pub use bridge::{ToolDefinition, ToolRegistry, ToolResult};
pub use manifest::{
    Author, HookEntry, HookMatcher, HooksConfig, LspServerConfig, LspServersConfig,
    McpServerConfig, McpServersConfig, PluginManifest,
};
pub use marketplace::{GitHubSource, MarketplaceEntry, MarketplaceManifest, PluginSource};
pub use plugin::LoadedPlugin;
pub use skill::{AgentDefinition, CommandDefinition, SkillDefinition};
pub use trust::TrustScore;
