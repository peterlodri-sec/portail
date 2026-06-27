use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Load .env file if present (local dev). NixOS/container deployments
/// inject env vars natively via systemd EnvironmentFile or container runtime.
fn try_load_dotenv() {
    if let Err(e) = dotenvy::dotenv() {
        // Not an error — .env may not exist in production
        tracing::debug!("dotenv: {e}");
    }
}

/// Profile-aware config path: debug = config.dev.toml, release = /etc/portail/config.toml
fn config_path(cli_path: Option<&Path>) -> Box<dyn AsRef<Path> + Send> {
    if let Some(path) = cli_path {
        return Box::new(path.to_path_buf());
    }
    if cfg!(debug_assertions) {
        Box::new(Path::new("portail.toml").to_path_buf())
    } else {
        Box::new(Path::new("/etc/portail/config.toml").to_path_buf())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_mcp_socket")]
    pub mcp_socket: String,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default = "default_cache_size")]
    pub cache_size: String,

    pub ai_gateway: Option<AiGatewayConfig>,
    pub mcp: Option<McpConfig>,
    pub cdn: Option<CdnConfig>,
    pub local_inference: Option<crate::local_inference::LocalInferenceConfig>,

    /// Upstream target templates — create, reuse, share
    #[serde(default)]
    pub targets: Vec<TargetConfig>,

    // ── v0.2 ──
    #[serde(default)]
    pub rate_limit: crate::rate_limit::RateLimitConfig,
    #[serde(default)]
    pub auth: crate::auth::AuthConfig,
    #[serde(default)]
    pub store: crate::store::StoreConfig,
    #[serde(default)]
    pub telemetry: crate::telemetry::TelemetryConfig,
}

fn default_listen() -> String {
    "0.0.0.0:8787".into()
}
fn default_mcp_socket() -> String {
    "/run/portail/mcp.sock".into()
}
fn default_cache_dir() -> String {
    "/var/cache/portail".into()
}
fn default_cache_size() -> String {
    "10g".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiGatewayConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_upstream")]
    pub upstream: String,
    pub default_provider: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_upstream() -> String {
    "http://127.0.0.1:4000".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mcp_socket")]
    pub socket_path: String,
    /// MCP backend: "python" (legacy uv sidecar) or "wasm" (v4+ Extism)
    #[serde(default)]
    pub backend: crate::mcp::McpBackend,
    pub server_registry: Option<Vec<McpServerEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetConfig {
    /// Template name (e.g. "anthropic-fast", "openai-gpt5")
    pub name: String,
    /// Provider: openai, anthropic, google, azure, custom
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Base URL for the API
    #[serde(default = "default_upstream")]
    pub base_url: String,
    /// API key (reference: env var name or inline)
    pub api_key: Option<String>,
    /// Models allowed for this target
    #[serde(default)]
    pub models: Vec<String>,
    /// Rate limit configuration (requests/sec)
    #[serde(default = "default_target_rps")]
    pub rps: f64,
    /// Extra headers to inject
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    /// Tags for organizing/sharing (e.g. "production", "staging", "shared-team")
    #[serde(default)]
    pub tags: Vec<String>,
    /// Human-readable description
    pub description: Option<String>,
}

fn default_provider() -> String {
    "openai".into()
}
fn default_target_rps() -> f64 {
    10.0
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            provider: default_provider(),
            base_url: default_upstream(),
            api_key: None,
            models: vec![],
            rps: default_target_rps(),
            headers: std::collections::HashMap::new(),
            tags: vec![],
            description: None,
        }
    }
}

/// Built-in target templates shipped with Portail
pub fn builtin_targets() -> Vec<TargetConfig> {
    vec![
        TargetConfig {
            name: "anthropic-fast".into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key: Some("$ANTHROPIC_API_KEY".into()),
            models: vec![
                "claude-sonnet-4-20250514".into(),
                "claude-haiku-3-20250313".into(),
            ],
            rps: 10.0,
            tags: vec!["built-in".into(), "fast".into()],
            description: Some("Anthropic Claude fast models (Sonnet, Haiku)".into()),
            ..Default::default()
        },
        TargetConfig {
            name: "anthropic-smart".into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key: Some("$ANTHROPIC_API_KEY".into()),
            models: vec!["claude-opus-4-20250514".into()],
            rps: 5.0,
            tags: vec!["built-in".into(), "smart".into()],
            description: Some("Anthropic Claude Opus — best quality, slower".into()),
            ..Default::default()
        },
        TargetConfig {
            name: "openai-gpt5".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: Some("$OPENAI_API_KEY".into()),
            models: vec!["gpt-5.4".into(), "gpt-5.2".into(), "gpt-5.1".into()],
            rps: 10.0,
            tags: vec!["built-in".into(), "fast".into()],
            description: Some("OpenAI GPT-5 series".into()),
            ..Default::default()
        },
        TargetConfig {
            name: "openai-o-series".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: Some("$OPENAI_API_KEY".into()),
            models: vec!["o3".into(), "o4-mini".into()],
            rps: 5.0,
            tags: vec!["built-in".into(), "reasoning".into()],
            description: Some("OpenAI o-series reasoning models".into()),
            ..Default::default()
        },
        TargetConfig {
            name: "google-gemini".into(),
            provider: "google".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api_key: Some("$GOOGLE_API_KEY".into()),
            models: vec!["gemini-2.5-flash".into(), "gemini-2.5-pro".into()],
            rps: 15.0,
            tags: vec!["built-in".into()],
            description: Some("Google Gemini models (flash + pro)".into()),
            ..Default::default()
        },
        TargetConfig {
            name: "openai-compatible".into(),
            provider: "openai".into(),
            base_url: "http://localhost:11434/v1".into(),
            api_key: None,
            models: vec!["llama3".into(), "qwen2.5".into(), "mistral".into()],
            rps: 30.0,
            tags: vec!["built-in".into(), "local".into()],
            description: Some("Local OpenAI-compatible (Ollama, vLLM, etc.)".into()),
            ..Default::default()
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerEntry {
    pub name: String,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    /// Tags for organizing (built-in, code, browser, search, etc.)
    #[serde(default)]
    pub tags: Vec<String>,
    /// Human-readable description
    pub description: Option<String>,
    /// Whether to start this server on boot
    #[serde(default = "default_true")]
    pub autostart: bool,
}

/// Built-in MCP server templates shipped with Portail
pub fn builtin_mcp_servers() -> Vec<McpServerEntry> {
    vec![
        McpServerEntry {
            name: "filesystem".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
                "/".into(),
            ]),
            tags: vec!["built-in".into(), "code".into()],
            description: Some("Local filesystem access — read, write, search files".into()),
            autostart: false,
            ..Default::default()
        },
        McpServerEntry {
            name: "github".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec![
                "-y".into(),
                "@modelcontextprotocol/server-github".into(),
            ]),
            tags: vec!["built-in".into(), "git".into()],
            description: Some("GitHub API — repos, PRs, issues, search".into()),
            autostart: false,
            ..Default::default()
        },
        McpServerEntry {
            name: "playwright".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec!["-y".into(), "@playwright/mcp".into()]),
            tags: vec!["built-in".into(), "browser".into()],
            description: Some(
                "Chrome DevTools / Playwright — browser automation, screenshots, inspection".into(),
            ),
            autostart: false,
            ..Default::default()
        },
        McpServerEntry {
            name: "fetch".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec![
                "-y".into(),
                "@modelcontextprotocol/server-fetch".into(),
            ]),
            tags: vec!["built-in".into(), "web".into()],
            description: Some("HTTP fetch — download web pages, APIs, JSON".into()),
            autostart: false,
            ..Default::default()
        },
        McpServerEntry {
            name: "brave-search".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec![
                "-y".into(),
                "@modelcontextprotocol/server-brave-search".into(),
            ]),
            tags: vec!["built-in".into(), "search".into()],
            description: Some("Web search via Brave Search API".into()),
            autostart: false,
            ..Default::default()
        },
        McpServerEntry {
            name: "sqlite".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec![
                "-y".into(),
                "@modelcontextprotocol/server-sqlite".into(),
                "./data.db".into(),
            ]),
            tags: vec!["built-in".into(), "database".into()],
            description: Some("SQLite database — query, schema, insert".into()),
            autostart: false,
            ..Default::default()
        },
        McpServerEntry {
            name: "sequential-thinking".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: Some(vec![
                "-y".into(),
                "@modelcontextprotocol/server-sequential-thinking".into(),
            ]),
            tags: vec!["built-in".into(), "reasoning".into()],
            description: Some("Step-by-step reasoning chains for complex problems".into()),
            autostart: false,
            ..Default::default()
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CdnConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_origin")]
    pub origin: String,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default = "default_cache_size")]
    pub cache_size: String,
    pub nats_url: Option<String>,
    #[serde(default)]
    pub domains: Vec<String>,
}

fn default_false() -> bool {
    false
}
fn default_origin() -> String {
    "http://127.0.0.1:9000".into()
}

impl Config {
    /// Load config from layered providers:
    ///   1. Default values (struct defaults)
    ///   2. TOML file (portail.toml in dev, /etc/portail/config.toml in release)
    ///   3. Environment variables prefixed with PORTAIL_ (e.g. PORTAIL_LISTEN)
    ///   4. CLI override path (if provided)
    pub fn load(cli_path: Option<&Path>) -> anyhow::Result<Self> {
        try_load_dotenv();

        let path = config_path(cli_path);
        let figment = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file(path.as_ref()))
            .merge(Env::prefixed("PORTAIL_").split("_"));

        Ok(figment.extract()?)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            mcp_socket: default_mcp_socket(),
            cache_dir: default_cache_dir(),
            cache_size: default_cache_size(),
            ai_gateway: None,
            mcp: None,
            cdn: None,
            local_inference: None,
            targets: vec![],
            rate_limit: crate::rate_limit::RateLimitConfig::default(),
            auth: crate::auth::AuthConfig::default(),
            store: crate::store::StoreConfig::default(),
            telemetry: crate::telemetry::TelemetryConfig::default(),
        }
    }
}

impl Default for AiGatewayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            upstream: default_upstream(),
            default_provider: None,
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            socket_path: default_mcp_socket(),
            backend: crate::mcp::McpBackend::default(),
            server_registry: Some(builtin_mcp_servers()),
        }
    }
}

impl Default for McpServerEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            transport: "stdio".into(),
            url: None,
            command: None,
            args: None,
            tags: vec![],
            description: None,
            autostart: true,
        }
    }
}

impl Default for CdnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            origin: default_origin(),
            cache_dir: default_cache_dir(),
            cache_size: default_cache_size(),
            nats_url: None,
            domains: vec![],
        }
    }
}
