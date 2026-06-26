pub mod dashboard;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "portail",
    about = "Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache",
    version,
    long_about = "Portail is a unified proxy and gateway for AI services, MCP tools, and CDN caching.\n\nRun without arguments to launch the interactive TUI dashboard."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to config file
    #[arg(short, long, default_value = "portail.toml")]
    pub config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Output format for non-interactive commands
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the proxy server
    Serve,

    /// Show current status and health
    Status,

    /// Stream agent lifecycle events
    Events {
        /// Number of recent events to show
        #[arg(short, long)]
        count: Option<usize>,

        /// Stream live events
        #[arg(short, long)]
        stream: bool,
    },

    /// Manage prompt injection hooks
    Hooks {
        #[command(subcommand)]
        action: HookAction,
    },

    /// Show or manage configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Show CDN cache statistics
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
    },

    /// Run health check
    Health,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HookAction {
    /// List all hooks
    List,
    /// Add a new hook (JSON inline or @file)
    Add {
        #[arg(short, long)]
        hook: String,
    },
    /// Delete a hook by ID
    Delete { id: String },
    /// Show hook details
    Show { id: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Validate configuration file
    Validate,
    /// Reload configuration (SIGHUP)
    Reload,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CacheAction {
    /// Show cache statistics
    Stats,
    /// Purge cache entries by prefix
    Purge { prefix: String },
    /// Show cache hit/miss ratios
    Ratio,
}

impl Cli {
    pub fn is_interactive(&self) -> bool {
        self.command.is_none()
    }

    pub fn is_server(&self) -> bool {
        matches!(self.command, Some(Commands::Serve))
    }
}
