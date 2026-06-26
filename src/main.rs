use clap::Parser;
use mimalloc::MiMalloc;
use portail::cdn;
use portail::cli;
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
            println!("showing {} recent events (streaming not yet implemented in CLI)", n);
            Ok(())
        }
        cli::Commands::Hooks { action } => {
            match action {
                cli::HookAction::List => println!("hooks: list (TODO: connect to server)"),
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
