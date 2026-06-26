use clap::Parser;
use mimalloc::MiMalloc;
use portail::cdn;
use portail::cli;
use portail::cli::complexity;
use portail::cli::guide;
use portail::cli::install as cli_install;
use portail::cli::learn;
use portail::cli::setup;
use portail::config::Config;
use portail::events::EventLog;
use portail::AppState;
use portail::mcp;
use std::sync::{Arc, RwLock};
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const MAX_EVENTS: usize = 2000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cli = cli::Cli::parse();

    // CLI mode: no subcommand → interactive TUI, subcommand → dispatch
    match &cli.command {
        None => {
            let mut dashboard = cli::dashboard::Dashboard::new();
            dashboard.run_tui()?;
            return Ok(());
        }
        Some(cli::Commands::Serve) => {} // fall through to server
        Some(cmd) => {
            dispatch_cli(cmd, &cli).await?;
            return Ok(());
        }
    }

    // ── Server mode ──────────────────────────────────────────────

    let handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");

    let config = Config::load(Some(&cli.config))?;
    let listen = config.listen.clone();
    tracing::info!(%listen, "portail starting");

    let event_log = Arc::new(EventLog::new(MAX_EVENTS));

    let cdn_cache = if let Some(cdn_cfg) = config.cdn.as_ref().filter(|c| c.enabled) {
        let cache = cdn::CacheManager::new(cdn_cfg);
        tokio::spawn({
            let c = Arc::clone(&cache);
            async move { cdn::stats_logger(c).await }
        });
        if let Some(ref nats_url) = cdn_cfg.nats_url {
            let nc = async_nats::connect(nats_url).await
                .expect("NATS connection failed");
            let sub = nc.subscribe("index.invalidated.>").await
                .expect("NATS subscribe failed");
            let c = Arc::clone(&cache);
            tokio::spawn(async move { cdn::purge_loop(sub, c).await });
            tracing::info!("NATS invalidation connected");
        }
        Some(cache)
    } else {
        None
    };

    if let Some(mcp_cfg) = config.mcp.as_ref().filter(|c| c.enabled) {
        mcp::start_sidecar(&mcp_cfg.socket_path).await?;
    }

    let state = Arc::new(AppState {
        config: RwLock::new(config),
        event_log: Arc::clone(&event_log),
        cdn_cache: cdn_cache.clone(),
        hooks: Arc::new(portail::hooks::HookStore::new()),
        a2a_tasks: Arc::new(portail::a2a::TaskStore::new()),
        dns_store: Arc::new(portail::dns::DnsStore::new()),
        doh_client: Some(Arc::new(portail::dns::DohClient::new(vec![
            "https://cloudflare-dns.com/dns-query".into(),
        ]))),
        network_isolation: Arc::new(portail::dns::NetworkIsolation::default()),
        tinyurl: Arc::new(portail::plugins::TinyUrlStore::new(portail::plugins::TinyUrlConfig::default())),
        trace_store: Arc::new(portail::plugins::TraceStore::new(10000)),
        redis_cache: Arc::new(portail::plugins::RedisCache::new(portail::plugins::RedisCacheConfig::default())),
        discovery: Arc::new(portail::discovery::DiscoveryStore::new(portail::discovery::DiscoveryConfig::default())),
        ebpf: Arc::new(portail::ebpf::EbpfManager::new(portail::ebpf::EbpfConfig::default())),
        iouring: Arc::new(portail::iouring::IoUringManager::new(portail::iouring::IoUringConfig::default())),
        dpdk: Arc::new(portail::dpdk::DpdkManager::new(portail::dpdk::DpdkConfig::default())),
        hyper: Arc::new(portail::hyper_engine::HyperManager::new(portail::hyper_engine::HyperConfig::default())),
        metrics_handle: handle,
    });

    tokio::spawn({
        let log = Arc::clone(&event_log);
        let cache = cdn_cache.clone();
        async move { portail::sentinel::run_sentinel(log, cache).await }
    });

    let app = portail::proxy::build_router(Arc::clone(&state));

    let sighup_state = Arc::clone(&state);
    let sighup_config_path = cli.config.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sig = signal(SignalKind::hangup())
            .expect("failed to install SIGHUP handler");
        loop {
            sig.recv().await;
            match Config::load(Some(&sighup_config_path)) {
                Ok(new) => {
                    *sighup_state.config.write().unwrap() = new;
                    tracing::info!("config reloaded on SIGHUP");
                }
                Err(e) => tracing::error!(error = %e, "config reload failed"),
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn dispatch_cli(cmd: &cli::Commands, cli: &cli::Cli) -> anyhow::Result<()> {
    match cmd {
        cli::Commands::Status => {
            println!("portail v{}", env!("CARGO_PKG_VERSION"));
            println!("config: {}", cli.config.display());
            Ok(())
        }
        cli::Commands::Events { count, stream: _ } => {
            let n = count.unwrap_or(20);
            println!("showing {} recent events", n);
            Ok(())
        }
        cli::Commands::Hooks { action } => {
            match action {
                cli::HookAction::List => println!("hooks: list"),
                cli::HookAction::Add { hook } => println!("hooks: add {}", hook),
                cli::HookAction::Delete { id } => println!("hooks: delete {}", id),
                cli::HookAction::Show { id } => println!("hooks: show {}", id),
            }
            Ok(())
        }
        cli::Commands::Config { action } => {
            match action {
                Some(cli::ConfigAction::Show) => println!("config: show"),
                Some(cli::ConfigAction::Validate) => println!("config: validate"),
                Some(cli::ConfigAction::Reload) => println!("config: reload"),
                None => println!("config: show (default)"),
            }
            Ok(())
        }
        cli::Commands::Cache { action } => {
            match action {
                Some(cli::CacheAction::Stats) => println!("cache: stats"),
                Some(cli::CacheAction::Purge { prefix }) => println!("cache: purge {}", prefix),
                Some(cli::CacheAction::Ratio) => println!("cache: ratio"),
                None => println!("cache: stats (default)"),
            }
            Ok(())
        }
        cli::Commands::Health => {
            println!("health: OK");
            Ok(())
        }
        cli::Commands::Complexity { format, output } => {
            let dir = std::path::Path::new("src");
            let output_path = output.as_deref();
            let report = complexity::analyze_and_report(dir, output_path)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", report);
            }
            Ok(())
        }
        cli::Commands::Install { method, dir } => {
            let install_method = match method {
                cli::InstallMethod::Auto => cli_install::InstallMethod::Auto,
                cli::InstallMethod::Cargo => cli_install::InstallMethod::Cargo,
                cli::InstallMethod::Nix => cli_install::InstallMethod::Nix,
                cli::InstallMethod::Binary => cli_install::InstallMethod::Binary,
            };
            cli_install::install(install_method, dir.as_deref())?;
            Ok(())
        }
        cli::Commands::Docs { open } => {
            println!("Generating documentation...");
            let status = std::process::Command::new("cargo")
                .args(["doc", "--no-deps", "--document-private-items"])
                .status()?;
            if !status.success() {
                anyhow::bail!("Failed to generate documentation");
            }
            if *open {
                #[cfg(target_os = "macos")]
                std::process::Command::new("open").arg("target/doc/portail/index.html").spawn()?;
                #[cfg(target_os = "linux")]
                std::process::Command::new("xdg-open").arg("target/doc/portail/index.html").spawn()?;
            }
            Ok(())
        }
        cli::Commands::Setup { non_interactive: _, domain, self_signed, headscale } => {
            let config = setup::SetupConfig {
                domain: domain.clone(),
                self_signed: *self_signed,
                headscale: *headscale,
                ..Default::default()
            };
            setup::run_setup(config)?;
            Ok(())
        }
        cli::Commands::Learn { topic } => {
            learn::run_learn(topic.as_deref())?;
            Ok(())
        }
        cli::Commands::Guide => {
            guide::run_guide()?;
            Ok(())
        }
        cli::Commands::Serve => unreachable!(),
    }
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = sigterm.recv() => tracing::info!("SIGTERM received"),
    }
}
