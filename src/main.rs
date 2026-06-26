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
    // ── v2.0: production panic hook ──
    portail::shutdown::install_panic_hook();

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

    // ── v1.1: self-healing config watcher ──
    let config_watcher = portail::config_watcher::ConfigWatcher::new(cli.config.clone());
    let config = config_watcher.init().await?;
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

    // ── v0.2: rate limiting ──
    let rate_limiter = if config.rate_limit.enabled {
        Some(portail::rate_limit::RateLimiter::new(config.rate_limit.clone()))
    } else {
        None
    };

    // ── v0.2: authentication ──
    let auth_state = if config.auth.enabled {
        Some(portail::auth::AuthState::new(config.auth.clone()))
    } else {
        None
    };

    // ── v0.2: persistent event store ──
    let event_store = if config.store.enabled {
        match portail::store::EventStore::open(config.store.clone()) {
            Ok(store) => {
                tracing::info!(path = %config.store.db_path, "persistent event store opened");
                Some(store)
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to open event store");
                None
            }
        }
    } else {
        None
    };

    // ── v0.2: OpenTelemetry OTLP ──
    let _otel_guard = portail::telemetry::init(&config.telemetry);

    let state = Arc::new(AppState {
        config: RwLock::new(config.clone()),
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
        ci_status: Arc::new(portail::ci::CiStatusStore::new(
            1000,
            std::env::var("PORTAIL_WEBHOOK_SECRET").ok()
                .or_else(|| {
                    let secret_file = dirs::config_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("portail")
                        .join("webhook-secret");
                    std::fs::read_to_string(secret_file).ok()
                }),
        )),
        metrics_handle: handle,
        rate_limiter,
        auth_state,
        event_store,
        session_store: portail::sessions::SessionStore::new(100),
        file_cache: portail::file_cache::FileCache::new(&portail::file_cache::FileCacheConfig::default()),
        config_watcher: config_watcher.clone(),
        supervisor: Arc::new(portail::supervisor::Supervisor::new(Arc::clone(&event_log))),
    });

    tokio::spawn({
        let log = Arc::clone(&event_log);
        let cache = cdn_cache.clone();
        async move { portail::sentinel::run_sentinel(log, cache).await }
    });

    // ── v2.0: NATS event bridge ──
    let nats_bridge = portail::nats_bridge::NatsEventBridge::new(&config).await;
    if nats_bridge.is_connected() {
        portail::nats_bridge::spawn_bridge(
            std::sync::Arc::new(nats_bridge),
            Arc::clone(&event_log),
        );
    }

    // ── v0.2: background agents ──
    tokio::spawn({
        let state = Arc::clone(&state);
        async move { portail::godfather::run_godfather(portail::godfather::GodfatherConfig::default(), state).await }
    });
    tokio::spawn({
        let state = Arc::clone(&state);
        async move { portail::nullclaw::run_nullclaw(portail::nullclaw::NullClawConfig::default(), state).await }
    });

    // ── v1.1: self-healing config (file watcher replaces SIGHUP) ──
    portail::config_watcher::spawn_watcher(config_watcher, Arc::clone(&state)).await;

    // Keep SIGHUP as fallback for manual reload signals
    {
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
                        sighup_state.config_watcher.health.store(true, std::sync::atomic::Ordering::Release);
                        tracing::info!("config reloaded on SIGHUP");
                    }
                    Err(e) => {
                        sighup_state.config_watcher.health.store(false, std::sync::atomic::Ordering::Release);
                        tracing::error!(error = %e, "config reload failed");
                    }
                }
            }
        });
    }

    let app = portail::proxy::build_router(Arc::clone(&state));

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
            let cfg = Config::load(Some(&cli.config))?;
            println!("listen: {}", cfg.listen);
            println!("rate_limit: {}", if cfg.rate_limit.enabled { "on" } else { "off" });
            println!("auth: {}", if cfg.auth.enabled { "on" } else { "off" });
            println!("store: {} (provider: {})", if cfg.store.enabled { "on" } else { "off" }, cfg.store.provider);
            // Check if server is running
            let url = format!("http://{}", cfg.listen);
            if let Ok(resp) = reqwest::blocking::get(format!("{}/healthz", url)) {
                println!("server: running (healthz: {})", resp.status());
            } else {
                println!("server: not running");
            }
            Ok(())
        }
        cli::Commands::Events { count: _, stream: _ } => {
            // Events require a running server. Print guidance.
            let cfg = Config::load(Some(&cli.config))?;
            let url = format!("http://{}/events", cfg.listen);
            if let Ok(resp) = reqwest::blocking::get(&url) {
                let body = resp.text().unwrap_or_default();
                let events: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                if let Some(arr) = events.as_array() {
                    for e in arr.iter().take(20) {
                        let agent = e["agent_id"].as_str().unwrap_or("-");
                        let typ = e["event_type"].as_str().unwrap_or("-");
                        println!("  [{}] {}", agent, typ);
                    }
                    println!("  ({})", arr.len());
                }
            } else {
                println!("server not running. Start with 'portail serve' then try again.");
            }
            Ok(())
        }
        cli::Commands::Hooks { action } => {
            let cfg = Config::load(Some(&cli.config))?;
            let base = format!("http://{}/hooks", cfg.listen);
            let client = reqwest::blocking::Client::new();
            match action {
                cli::HookAction::List => {
                    match client.get(&base).send() {
                        Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                        Err(_) => println!("server not running. Start with 'portail serve'."),
                    }
                }
                cli::HookAction::Add { hook } => {
                    let body: serde_json::Value = serde_json::from_str(hook)?;
                    match client.post(&base).json(&body).send() {
                        Ok(resp) => println!("created: {}", resp.status()),
                        Err(_) => println!("server not running."),
                    }
                }
                cli::HookAction::Delete { id } => {
                    match client.delete(format!("{}/{}", base, id)).send() {
                        Ok(resp) => println!("deleted: {}", resp.status()),
                        Err(_) => println!("server not running."),
                    }
                }
                cli::HookAction::Show { id } => {
                    match client.get(format!("{}/{}", base, id)).send() {
                        Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                        Err(_) => println!("server not running."),
                    }
                }
            }
            Ok(())
        }
        cli::Commands::Config { action } => {
            match action {
                Some(cli::ConfigAction::Show) => {
                    let cfg = Config::load(Some(&cli.config))?;
                    println!("{}", toml::to_string_pretty(&cfg)?);
                }
                Some(cli::ConfigAction::Validate) => {
                    match Config::load(Some(&cli.config)) {
                        Ok(cfg) => {
                            println!("valid: {} ports, rate_limit={}, auth={}",
                                cfg.listen, cfg.rate_limit.enabled, cfg.auth.enabled);
                        }
                        Err(e) => {
                            eprintln!("invalid: {}", e);
                            anyhow::bail!("config validation failed");
                        }
                    }
                }
                Some(cli::ConfigAction::Reload) => println!("config reload: send SIGHUP or modify the file (auto-reload enabled)"),
                Some(cli::ConfigAction::Rollback { version }) => {
                    use portail::config_watcher::PersistedHistory;
                    if let Some(hist) = PersistedHistory::load(&cli.config) {
                        if let Some(entry) = hist.versions.iter().find(|v| v.version == *version) {
                            std::fs::write(&cli.config, &entry.config_json)?;
                            println!("rolled back to version {} (from {})", version, entry.loaded_at);
                        } else {
                            println!("version {} not found. Available versions:", version);
                            for v in &hist.versions {
                                println!("  v{} — {}", v.version, v.loaded_at);
                            }
                        }
                    } else {
                        println!("no config history found (file will be created on first auto-reload)");
                    }
                }
                Some(cli::ConfigAction::History) => {
                    use portail::config_watcher::PersistedHistory;
                    if let Some(hist) = PersistedHistory::load(&cli.config) {
                        println!("Config version history (last {} entries):", hist.versions.len());
                        for v in &hist.versions {
                            println!("  v{} — {}", v.version, v.loaded_at);
                        }
                    } else {
                        println!("no config history yet");
                    }
                }
                None => {
                    let cfg = Config::load(Some(&cli.config))?;
                    println!("{}", toml::to_string_pretty(&cfg)?);
                }
            }
            Ok(())
        }
        cli::Commands::Cache { action } => {
            let cfg = Config::load(Some(&cli.config))?;
            let base = format!("http://{}/cache", cfg.listen);
            let client = reqwest::blocking::Client::new();
            match action {
                Some(cli::CacheAction::Stats) => {
                    match client.get(&base).send() {
                        Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                        Err(_) => println!("server not running. Start with 'portail serve'."),
                    }
                }
                Some(cli::CacheAction::Purge { prefix }) => {
                    match client.post(format!("{}/flush", base)).json(&serde_json::json!({"prefix": prefix})).send() {
                        Ok(resp) => println!("purged: {}", resp.status()),
                        Err(_) => println!("server not running."),
                    }
                }
                Some(cli::CacheAction::Ratio) => {
                    match client.get(&base).send() {
                        Ok(resp) => {
                            let v: serde_json::Value = serde_json::from_str(&resp.text().unwrap_or_default()).unwrap_or_default();
                            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                        }
                        Err(_) => println!("server not running."),
                    }
                }
                None => {
                    match client.get(&base).send() {
                        Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                        Err(_) => println!("server not running."),
                    }
                }
            }
            Ok(())
        }
        cli::Commands::Health => {
            let cfg = Config::load(Some(&cli.config))?;
            let url = format!("http://{}/healthz", cfg.listen);
            match reqwest::blocking::get(&url) {
                Ok(resp) if resp.status().is_success() => println!("server is healthy (healthz: {})", resp.status()),
                Ok(resp) => println!("server is running but unhealthy (healthz: {})", resp.status()),
                Err(_) => println!("server is not running on {}. Start with 'portail serve'.", cfg.listen),
            }
            Ok(())
        }
        cli::Commands::Complexity { format, output, ci, ci_report_path } => {
            let ci = *ci;
            let dir = std::path::Path::new("src");
            let report = match complexity::analyze_directory(dir) {
                Ok(r) => r,
                Err(e) => {
                    if ci {
                        // CI mode: write error report to file, always exit 0
                        let err_report = format!("error: {}", e);
                        std::fs::write(&ci_report_path, &err_report).ok();
                        println!("{}", err_report);
                        return Ok(());
                    }
                    return Err(e.into());
                }
            };
            let report_text = complexity::generate_report(&report);

            // CI mode: write TOML report to file, always exit 0
            // Daily-only: skip if report is from today
            if ci {
                let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                if ci_report_path.exists() {
                    if let Ok(meta) = std::fs::metadata(&ci_report_path) {
                        if let Ok(modified) = meta.modified() {
                            let modified_date = chrono::DateTime::<chrono::Utc>::from(modified)
                                .format("%Y-%m-%d")
                                .to_string();
                            if modified_date == today {
                                println!("complexity-ci: report already generated today ({}) — skipping", today);
                                return Ok(());
                            }
                        }
                    }
                }
                let toml_report = toml::to_string_pretty(&report).unwrap_or_default();
                std::fs::write(&ci_report_path, format!("# generated: {}\n{}", today, toml_report)).ok();
                // Print summary to stdout for CI log
                println!("complexity-ci: wrote report to {}", ci_report_path.display());
                println!("  files: {} | functions: {} | annotations: {}",
                    report.files_scanned, report.total_functions, report.total_annotations);
                for (c, n) in &report.distribution {
                    println!("  {}: {}", c, n);
                }
                return Ok(());
            }

            if let Some(path) = output.as_deref() {
                std::fs::write(path, &report_text)?;
            }
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", report_text);
            }
            Ok(())
        }
        cli::Commands::DriftDetect { command, ci } => {
            portail::drift::run(command, *ci)?;
            Ok(())
        }
        cli::Commands::SpecVerify { command, ci } => {
            portail::spec_verify::run(command, *ci)?;
            Ok(())
        }
        cli::Commands::FuzzRoute { url, ci } => {
            if *ci {
                portail::fuzz_route::ci_run(url)?;
            } else {
                let report = portail::fuzz_route::run(url)?;
                println!(
                    "fuzz-route: {} probes | {} passed | {} errors | {} crashes",
                    report.total_probes, report.passed, report.errored, report.crashed
                );
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
        cli::Commands::Amberify { input, output } => {
            cli::amberify::process_path(input, output.as_deref())?;
            Ok(())
        }
        cli::Commands::Init => {
            cli::init::run_init()?;
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
