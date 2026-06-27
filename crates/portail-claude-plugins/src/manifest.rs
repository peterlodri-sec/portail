//! Plugin manifest types — `.claude-plugin/plugin.json`, `hooks/hooks.json`,
//! `.mcp.json`, `.lsp.json`.
//!
//! Types mirror the Python `claude-code-plugins-sdk` exactly.

use serde::{Deserialize, Serialize};

// ─── plugin.json ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,

    // Component path fields — each accepts a path string or list of paths
    #[serde(default, alias = "commands")]
    pub commands: Option<ComponentPath>,
    #[serde(default, alias = "agents")]
    pub agents: Option<ComponentPath>,
    #[serde(default, alias = "skills")]
    pub skills: Option<ComponentPath>,
    #[serde(default)]
    pub hooks: Option<serde_json::Value>,
    #[serde(default, alias = "mcpServers")]
    pub mcp_servers: Option<serde_json::Value>,
    #[serde(default, alias = "lspServers")]
    pub lsp_servers: Option<serde_json::Value>,
    #[serde(default, alias = "outputStyles")]
    pub output_styles: Option<ComponentPath>,
}

/// A component path can be a single string or a list of strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComponentPath {
    Single(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Author {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

// ─── hooks.json ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub hooks: std::collections::HashMap<String, Vec<HookMatcher>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    #[serde(default)]
    pub matcher: Option<String>,
    pub hooks: Vec<HookEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    #[serde(rename = "type")]
    pub hook_type: String, // "command" | "prompt" | "agent" | "http" | "mcp_tool"
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub async_: Option<bool>,
    #[serde(default, alias = "statusMessage")]
    pub status_message: Option<String>,
    #[serde(default)]
    pub once: Option<bool>,
    #[serde(default, alias = "if")]
    pub if_: Option<String>,
}

// ─── .mcp.json ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServersConfig {
    #[serde(default, alias = "mcpServers")]
    pub mcp_servers: std::collections::HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    pub server_type: Option<String>, // "stdio" | "http" | "sse"
    #[serde(default)]
    pub url: Option<String>,
}

// ─── .lsp.json ────────────────────────────────────────────────────

pub type LspServersConfig = std::collections::HashMap<String, LspServerConfig>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LspServerConfig {
    pub command: String,
    #[serde(default, alias = "extensionToLanguage")]
    pub extension_to_language: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub transport: Option<String>, // "stdio" | "socket"
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default, alias = "initializationOptions")]
    pub initialization_options: Option<serde_json::Value>,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    #[serde(default, alias = "workspaceFolder")]
    pub workspace_folder: Option<String>,
    #[serde(default, alias = "startupTimeout")]
    pub startup_timeout: Option<u64>,
    #[serde(default, alias = "shutdownTimeout")]
    pub shutdown_timeout: Option<u64>,
    #[serde(default, alias = "restartOnCrash")]
    pub restart_on_crash: Option<bool>,
    #[serde(default, alias = "maxRestarts")]
    pub max_restarts: Option<u32>,
}
