pub mod amberify;
pub mod complexity;
pub mod dashboard;
pub mod dev;
pub mod doctor;
pub mod guide;
pub mod init;
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

    /// Developer/contributor entry point — dev dashboard, check, test, lint, build, audit
    Dev {
        #[command(subcommand)]
        action: DevAction,
    },

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

    /// Compare route table against golden spec
    SpecVerify {
        #[command(subcommand)]
        command: crate::spec_verify::SpecCommand,

        /// CI mode: write report, always exit 0
        #[arg(long)]
        ci: bool,
    },

    /// Fuzz all routes with malformed input (crash detector)
    FuzzRoute {
        /// Proxy URL to fuzz
        #[arg(long, default_value = "http://localhost:8787")]
        url: String,

        /// CI mode: write report, exit 1 on crash
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

    /// Interactive configuration generator (wizard)
    Init,

    /// System compatibility check
    Doctor,

    /// Manage upstream target templates (list, export, share)
    Target {
        #[command(subcommand)]
        action: TargetAction,
    },

    /// Manage built-in MCP servers (list, install, config)
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Manage .vaked plugins (list, load, lower, build)
    Vaked {
        #[command(subcommand)]
        action: VakedAction,
    },

    /// Process Interception Tracker — watch /proc, log all processes
    Pit {
        /// One-shot scan instead of continuous watch
        #[arg(long)]
        scan: bool,
    },

    /// Release audit: verify binaries, generate SBOM + report
    ReleaseAudit {
        /// Directory containing release artifacts
        #[arg(short, long, default_value = "dist")]
        dir: PathBuf,

        /// Release version string
        #[arg(short, long)]
        version: String,

        /// Output directory for reports
        #[arg(short, long)]
        out: Option<PathBuf>,
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
    /// Rollback to a previous config version
    Rollback {
        /// Version number (1-indexed, from 'config history')
        version: u64,
    },
    /// Show config version history
    History,
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

#[derive(Subcommand, Debug, Clone)]
pub enum TargetAction {
    /// List all available target templates (built-in + configured)
    List,
    /// Export a target as a shareable JSON snippet
    Export { name: String },
    /// Show default built-in targets
    Builtins,
}

#[derive(Subcommand, Debug, Clone)]
pub enum DevAction {
    /// Interactive dev dashboard (TUI)
    #[command(name = "dashboard")]
    Dashboard,
    /// Cargo check across all allocator variants
    Check,
    /// Run tests (all suites)
    Test,
    /// Lint: clippy + fmt check
    Lint,
    /// Build release binary
    Build {
        /// Use fat LTO instead of thin (slower, smaller binary)
        #[arg(long)]
        max: bool,
    },
    /// Full release audit: verify, SBOM, report, stamp
    Audit {
        /// Release version string
        #[arg(short, long)]
        version: String,
    },
    /// Run the full CI pipeline (check → lint → test → audit)
    Ci {
        /// Release version for audit step
        #[arg(short, long)]
        version: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum McpAction {
    /// List all MCP servers (built-in + registry)
    List,
    /// Show info about a specific MCP server
    Info { name: String },
    /// Generate configuration block for a built-in MCP server
    Config { name: String },
    /// List built-in MCP server templates
    Builtins,
}

#[derive(Subcommand, Debug, Clone)]
pub enum VakedAction {
    /// List loaded .vaked plugins
    List,
    /// Load a .vaked file
    Load { path: PathBuf },
    /// Show the lowered Nix/NixOS output
    Lower { path: PathBuf },
    /// Build a .vaked plugin to WASM
    Build { path: PathBuf },
}
