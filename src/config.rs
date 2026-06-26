use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "portail", about = "Unified proxy/gateway: AI + MCP + CDN")]
pub struct Cli {
    #[arg(long, env = "PORTAIL_CONFIG")]
    pub config: Option<PathBuf>,

    #[arg(long, env = "PORTAIL_LISTEN")]
    pub listen: Option<String>,

    #[arg(long, env = "PORTAIL_MCP_SOCKET")]
    pub mcp_socket: Option<String>,

    #[arg(long, env = "PORTAIL_CACHE_DIR")]
    pub cache_dir: Option<String>,

    #[arg(long, env = "PORTAIL_CACHE_SIZE")]
    pub cache_size: Option<String>,

    #[arg(long, env = "PORTAIL_ENABLE_AI_GATEWAY")]
    pub enable_ai_gateway: Option<bool>,

    #[arg(long, env = "PORTAIL_ENABLE_MCP")]
    pub enable_mcp: Option<bool>,

    #[arg(long, env = "PORTAIL_ENABLE_CDN")]
    pub enable_cdn: Option<bool>,

    #[arg(long, env = "PORTAIL_AI_UPSTREAM")]
    pub ai_upstream: Option<String>,

    #[arg(long, env = "PORTAIL_CDN_ORIGIN")]
    pub cdn_origin: Option<String>,

    #[arg(long, env = "PORTAIL_NATS_URL")]
    pub nats_url: Option<String>,
}

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
    pub fn load(cli: &Cli) -> anyhow::Result<Self> {
        let mut cfg: Config = if let Some(ref path) = cli.config {
            if path.exists() {
                let raw = std::fs::read_to_string(path)?;
                toml::from_str(&raw)?
            } else {
                Config::default()
            }
        } else {
            Config::default()
        };

        if let Some(v) = &cli.listen { cfg.listen = v.clone(); }
        if let Some(v) = &cli.mcp_socket { cfg.mcp_socket = v.clone(); }
        if let Some(v) = &cli.cache_dir { cfg.cache_dir = v.clone(); }
        if let Some(v) = &cli.cache_size { cfg.cache_size = v.clone(); }
        if let Some(v) = cli.enable_ai_gateway {
            if v {
                cfg.ai_gateway.get_or_insert_with(AiGatewayConfig::default);
            } else if let Some(ref mut g) = cfg.ai_gateway {
                g.enabled = false;
            }
        }
        if let Some(v) = cli.enable_mcp {
            if v {
                cfg.mcp.get_or_insert_with(McpConfig::default);
            } else if let Some(ref mut m) = cfg.mcp {
                m.enabled = false;
            }
        }
        if let Some(v) = cli.enable_cdn {
            if v {
                cfg.cdn.get_or_insert_with(CdnConfig::default);
            } else if let Some(ref mut c) = cfg.cdn {
                c.enabled = false;
            }
        }
        if let Some(v) = &cli.ai_upstream {
            cfg.ai_gateway.get_or_insert_with(AiGatewayConfig::default).upstream = v.clone();
        }
        if let Some(v) = &cli.cdn_origin {
            cfg.cdn.get_or_insert_with(CdnConfig::default).origin = v.clone();
        }
        if let Some(v) = &cli.nats_url {
            cfg.cdn.get_or_insert_with(CdnConfig::default).nats_url = Some(v.clone());
        }
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
