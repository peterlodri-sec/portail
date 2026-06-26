use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn default_listen() -> String { "0.0.0.0:8787".into() }
fn default_mcp_socket() -> String { "/run/portail/mcp.sock".into() }
fn default_cache_dir() -> String { "/var/cache/portail".into() }
fn default_cache_size() -> String { "10g".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiGatewayConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_upstream")]
    pub upstream: String,
    pub default_provider: Option<String>,
}

fn default_true() -> bool { true }
fn default_upstream() -> String { "http://127.0.0.1:4000".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mcp_socket")]
    pub socket_path: String,
    pub server_registry: Option<Vec<McpServerEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn default_false() -> bool { false }
fn default_origin() -> String { "http://127.0.0.1:9000".into() }

impl Config {
    pub fn load(path: Option<&std::path::Path>) -> anyhow::Result<Self> {
        let cfg: Config = if let Some(path) = path {
            if path.exists() {
                let raw = std::fs::read_to_string(path)?;
                toml::from_str(&raw)?
            } else {
                Config::default()
            }
        } else {
            Config::default()
        };
        Ok(cfg)
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
            rate_limit: crate::rate_limit::RateLimitConfig::default(),
            auth: crate::auth::AuthConfig::default(),
            store: crate::store::StoreConfig::default(),
            telemetry: crate::telemetry::TelemetryConfig::default(),
        }
    }
}

impl Default for AiGatewayConfig {
    fn default() -> Self {
        Self { enabled: true, upstream: default_upstream(), default_provider: None }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: true, socket_path: default_mcp_socket(), server_registry: None }
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
