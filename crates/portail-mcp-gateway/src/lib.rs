//! portail-mcp-gateway — Embedded MCP Gateway for Portail.
//!
//! Wraps the `mcp-gateway` crate (https://github.com/MikkoParkkola/mcp-gateway)
//! as a native Rust MCP routing layer, replacing the Python/LiteLLM sidecar.
//!
//! Architecture:
//! ```text
//! Portail proxy ──► portail-mcp-gateway (embedded Gateway)
//! │                       │
//! │              ┌────────┴────────┐
//! │              │  mcp-gateway    │
//! │              │  - MCP routing  │
//! │              │  - Meta-MCP     │
//! │              │  - Capabilities │
//! │              │  - Backends     │
//! │              └─────────────────┘
//! │
//! └──► zeroclaw (sidecar agent)
//!     - Dashboard + channels
//!     - Telegram/Discord/Matrix
//!     - Webhook ingress
//! ```

use tracing::{error, info};

/// Configuration for the embedded MCP gateway.
#[derive(Debug, Clone)]
pub struct McpGatewayConfig {
    /// Host to bind the MCP gateway server to.
    pub host: String,
    /// Port to bind the MCP gateway server to.
    pub port: u16,
    /// Path to MCP gateway config file (YAML/TOML).
    /// When None, uses default config.
    pub config_path: Option<String>,
}

impl Default for McpGatewayConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 39400,
            config_path: None,
        }
    }
}

/// Launch the embedded MCP gateway server on the given host:port.
///
/// Returns a handle that can be awaited or cancelled.
pub fn launch_gateway(config: McpGatewayConfig) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        info!(
            host = %config.host,
            port = config.port,
            "Starting embedded MCP gateway"
        );

        // Build mcp-gateway config from our own config
        let gateway_config = match &config.config_path {
            Some(path) => {
                // Load from file
                let path_ref: Option<&std::path::Path> = Some(std::path::Path::new(path));
                mcp_gateway::config::Config::load(path_ref)
                    .map_err(|e| anyhow::anyhow!("failed to load MCP gateway config: {e}"))?
            }
            None => {
                // Use default config with the right bind address
                let mut cfg = mcp_gateway::config::Config::load(None).map_err(|e| {
                    anyhow::anyhow!("failed to create default MCP gateway config: {e}")
                })?;
                cfg.server.host = config.host.clone();
                cfg.server.port = config.port;
                cfg
            }
        };

        // Create and run the gateway
        match mcp_gateway::gateway::Gateway::new(gateway_config).await {
            Ok(gateway) => {
                info!("MCP gateway initialised, starting server");
                if let Err(e) = gateway.run().await {
                    error!(error = %e, "MCP gateway server error");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to initialise MCP gateway");
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gateway_config_default_valid() {
        let cfg = McpGatewayConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 39400);
    }
}
