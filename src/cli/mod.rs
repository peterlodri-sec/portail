pub mod amberify;
pub mod complexity;
pub mod dashboard;
pub mod guide;
pub mod install;
pub mod learn;
pub mod setup;

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

    /// Analyze time complexity (Big O) across the codebase
    Complexity {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,

        /// Save report to file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// CI mode: write report to file, always exit 0, never fail
        #[arg(long)]
        ci: bool,

        /// CI report path (default: complexity-report.toml)
        #[arg(long, default_value = "complexity-report.toml")]
        ci_report_path: PathBuf,
    },

    /// Capture/replay production traffic for regression detection
    DriftDetect {
        #[command(subcommand)]
        command: crate::drift::DriftCommand,

        /// CI mode: write report to file, always exit 0
        #[arg(long)]
        ci: bool,
    },

    /// Install portail (binary, cargo, or nix)
    Install {
        /// Installation method
        #[arg(long, default_value = "auto")]
        method: InstallMethod,

        /// Installation directory (for binary install)
        #[arg(long)]
        dir: Option<PathBuf>,
    },

    /// Generate documentation
    Docs {
        /// Open docs in browser after generation
        #[arg(long)]
        open: bool,
    },

    /// Setup portail: domain, DNS, certificates
    Setup {
        /// Skip interactive prompts and use defaults
        #[arg(long)]
        non_interactive: bool,

        /// Domain name (e.g., portail.example.com)
        #[arg(long)]
        domain: Option<String>,

        /// Use self-signed certificates instead of Let's Encrypt
        #[arg(long)]
        self_signed: bool,

        /// Setup Headscale for mesh networking
        #[arg(long)]
        headscale: bool,
    },

    /// Learn about networking and security concepts
    Learn {
        /// Topic to learn about (dns, tcp, tls, doh, vpn, proxy, firewall, dnssec, zero-trust, headscale)
        topic: Option<String>,
    },

    /// Interactive guide for branch protection setup
    Guide,

    /// Convert shell scripts to Amber language
    Amberify {
        /// Input file or directory
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum InstallMethod {
    Auto,
    Cargo,
    Nix,
    Binary,
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
