use clap::Parser;
use portail::AppState;
use portail::cdn;
use portail::cli;
use portail::cli::complexity;
use portail::cli::guide;
use portail::cli::install as cli_install;
use portail::cli::learn;
use portail::cli::setup;
use portail::config::Config;
use portail::events::EventLog;
use portail::mcp;
use std::sync::{Arc, RwLock};

// Allocator selection:
//   default    → mimalloc (fast general-purpose)
//   jemalloc   → cargo build --features jemalloc  (better for high-concurrency)
//   system     → cargo build --cfg portail_system_alloc  (use Rust's default alloc)
#[cfg(not(any(feature = "jemalloc", feature = "portail_system_alloc")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const MAX_EVENTS: usize = 2000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── v2.0: production panic hook ──
    portail::shutdown::install_panic_hook();

    // ── v2.x: non-blocking JSON tracing (SOTA) ──
    let (_guard, _log_dir) = portail::telemetry::init_logging();

    let cli = cli::Cli::parse();

    // CLI mode: no subcommand → print help, subcommand → dispatch
    match &cli.command {
        None => {
            let mut cmd = <cli::Cli as clap::CommandFactory>::command();
            let _ = cmd.print_help();
            println!();
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
            let nc = async_nats::connect(nats_url)
                .await
                .expect("NATS connection failed");
            let sub = nc
                .subscribe("index.invalidated.>")
                .await
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
        let config_json = mcp_cfg
            .server_registry
            .as_ref()
            .map(|r| serde_json::to_string(r).unwrap_or_default());
        mcp::start_sidecar(
            &mcp_cfg.socket_path,
            config_json.as_deref(),
            &mcp_cfg.backend,
        )
        .await?;
    }

    // ── v0.2: rate limiting ──
    let rate_limiter = if config.rate_limit.enabled {
        Some(portail::rate_limit::RateLimiter::new(
            config.rate_limit.clone(),
        ))
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
        match portail::store::EventStore::open(config.store.clone()).await {
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
        a2a_registry: Arc::new(portail::a2a::registry::AgentRegistry::new()),
        dns_store: Arc::new(portail::dns::DnsStore::new()),
        doh_client: Some(Arc::new(portail::dns::DohClient::new(vec![
            "https://cloudflare-dns.com/dns-query".into(),
        ]))),
        network_isolation: Arc::new(portail::dns::NetworkIsolation::default()),
        tinyurl: Arc::new(portail::plugins::TinyUrlStore::new(
            portail::plugins::TinyUrlConfig::default(),
        )),
        trace_store: Arc::new(portail::plugins::TraceStore::new(10000)),
        redis_cache: Arc::new(portail::plugins::RedisCache::new(
            portail::plugins::RedisCacheConfig::default(),
        )),
        discovery: Arc::new(portail::discovery::DiscoveryStore::new(
            portail::discovery::DiscoveryConfig::default(),
        )),
        ci_status: Arc::new(portail::ci::CiStatusStore::new(
            1000,
            std::env::var("PORTAIL_WEBHOOK_SECRET").ok().or_else(|| {
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
        file_cache: portail::file_cache::FileCache::new(
            &portail::file_cache::FileCacheConfig::default(),
        ),
        config_watcher: config_watcher.clone(),
        supervisor: Arc::new(portail::supervisor::Supervisor::new(Arc::clone(&event_log))),
        plugin_registry: portail::plugin_hooks::init_plugin_registry(std::path::Path::new("vaked")),
        loop_manager: std::sync::Arc::new(loop_state_manager::LoopStateManager::new(env!(
            "CARGO_PKG_VERSION"
        ))),
        loop_runner: loopeng::SharedLoopEngine::new(loopeng::LoopEngineConfig {
            name: "portail-server".into(),
            token_budget: Some(100_000),
            escalate_after_failures: 3,
            circuit_breaker_threshold: 5,
            ..Default::default()
        }),
        inference_engine: config
            .local_inference
            .as_ref()
            .filter(|c| c.enabled)
            .map(|c| Arc::new(portail::local_inference::InferenceEngine::new(c.clone()))),
        pkg_ctx_memory: tokio::sync::Mutex::new(pkg_ctx::memory::PkgCtxMemory::new()?),
    });

    // ── v2.0: session TTL eviction (1h) ──
    state.session_store.clone().spawn_eviction(3600);

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

    // ── v2.x: loop engine runner ──
    state.loop_runner.with_engine(|e| {
        e.add_schedule(loopeng::Schedule {
            name: "pkg-ctx-index".into(),
            cadence_secs: 86400,
            pattern: "daily-reindex".into(),
            max_iterations: None,
            enabled: true,
        });
        e.add_schedule(loopeng::Schedule {
            name: "health-check".into(),
            cadence_secs: 300,
            pattern: "health".into(),
            max_iterations: None,
            enabled: true,
        });
    });

    // ── v2.x: pkg-ctx memory (in-memory docs, auto-save on drop) ──
    let pkg_dir = dirs::data_dir()
        .map(|d| d.join("portail").join(pkg_ctx::PKG_DIR))
        .unwrap_or_else(|| std::path::Path::new(pkg_ctx::PKG_DIR).to_path_buf());
    std::fs::create_dir_all(&pkg_dir).ok();
    *state.pkg_ctx_memory.lock().await = pkg_ctx::memory::PkgCtxMemory::load_or_create(&pkg_dir)?;

    // ── v0.2: background agents ──
    tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            portail::godfather::run_godfather(portail::godfather::GodfatherConfig::default(), state)
                .await
        }
    });

    // ── v2.x: ADK-Rust CI agent runner ──
    {
        let mut agents = std::collections::HashMap::new();
        let spec_config = portail_agents::ci::spec_verify::SpecVerifyConfig::default();
        if let Ok(agent) = portail_agents::ci::spec_verify::build_spec_verify_agent(&spec_config) {
            agents.insert("spec-verify".into(), agent);
        }
        portail_agents::ci::runner::spawn_runner(
            portail_agents::ci::runner::CiRunnerConfig::default(),
            agents,
        );
    }

    // ── v2.x: NullClaw fleet heartbeat ──
    {
        let nullclaw_config = portail_agents::nullclaw::NullClawConfig::default();
        tokio::spawn(async move {
            portail_agents::nullclaw::run_nullclaw_loop(nullclaw_config).await;
        });
    }

    // ── v1.1: self-healing config (file watcher replaces SIGHUP) ──
    portail::config_watcher::spawn_watcher(config_watcher, Arc::clone(&state)).await;

    // Keep SIGHUP as fallback for manual reload signals
    {
        let sighup_state = Arc::clone(&state);
        let sighup_config_path = cli.config.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            let mut sig = signal(SignalKind::hangup()).expect("failed to install SIGHUP handler");
            loop {
                sig.recv().await;
                match Config::load(Some(&sighup_config_path)) {
                    Ok(new) => {
                        *sighup_state.config.write().unwrap() = new;
                        sighup_state
                            .config_watcher
                            .health
                            .store(true, std::sync::atomic::Ordering::Release);
                        tracing::info!("config reloaded on SIGHUP");
                    }
                    Err(e) => {
                        sighup_state
                            .config_watcher
                            .health
                            .store(false, std::sync::atomic::Ordering::Release);
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
            println!(
                "rate_limit: {}",
                if cfg.rate_limit.enabled { "on" } else { "off" }
            );
            println!("auth: {}", if cfg.auth.enabled { "on" } else { "off" });
            println!(
                "store: {} (provider: {})",
                if cfg.store.enabled { "on" } else { "off" },
                cfg.store.provider
            );
            // Check if server is running
            let url = format!("http://{}", cfg.listen);
            if let Ok(resp) = reqwest::blocking::get(format!("{}/healthz", url)) {
                println!("server: running (healthz: {})", resp.status());
            } else {
                println!("server: not running");
            }
            Ok(())
        }
        cli::Commands::Events {
            count: _,
            stream: _,
        } => {
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
                cli::HookAction::List => match client.get(&base).send() {
                    Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                    Err(_) => println!("server not running. Start with 'portail serve'."),
                },
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
                Some(cli::ConfigAction::Validate) => match Config::load(Some(&cli.config)) {
                    Ok(cfg) => {
                        println!(
                            "valid: {} ports, rate_limit={}, auth={}",
                            cfg.listen, cfg.rate_limit.enabled, cfg.auth.enabled
                        );
                    }
                    Err(e) => {
                        eprintln!("invalid: {}", e);
                        anyhow::bail!("config validation failed");
                    }
                },
                Some(cli::ConfigAction::Reload) => {
                    println!("config reload: send SIGHUP or modify the file (auto-reload enabled)")
                }
                Some(cli::ConfigAction::Rollback { version }) => {
                    use portail::config_watcher::PersistedHistory;
                    if let Some(hist) = PersistedHistory::load(&cli.config) {
                        if let Some(entry) = hist.versions.iter().find(|v| v.version == *version) {
                            std::fs::write(&cli.config, &entry.config_json)?;
                            println!(
                                "rolled back to version {} (from {})",
                                version, entry.loaded_at
                            );
                        } else {
                            println!("version {} not found. Available versions:", version);
                            for v in &hist.versions {
                                println!("  v{} — {}", v.version, v.loaded_at);
                            }
                        }
                    } else {
                        println!(
                            "no config history found (file will be created on first auto-reload)"
                        );
                    }
                }
                Some(cli::ConfigAction::History) => {
                    use portail::config_watcher::PersistedHistory;
                    if let Some(hist) = PersistedHistory::load(&cli.config) {
                        println!(
                            "Config version history (last {} entries):",
                            hist.versions.len()
                        );
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
                Some(cli::CacheAction::Stats) => match client.get(&base).send() {
                    Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                    Err(_) => println!("server not running. Start with 'portail serve'."),
                },
                Some(cli::CacheAction::Purge { prefix }) => {
                    match client
                        .post(format!("{}/flush", base))
                        .json(&serde_json::json!({"prefix": prefix}))
                        .send()
                    {
                        Ok(resp) => println!("purged: {}", resp.status()),
                        Err(_) => println!("server not running."),
                    }
                }
                Some(cli::CacheAction::Ratio) => match client.get(&base).send() {
                    Ok(resp) => {
                        let v: serde_json::Value =
                            serde_json::from_str(&resp.text().unwrap_or_default())
                                .unwrap_or_default();
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                    }
                    Err(_) => println!("server not running."),
                },
                None => match client.get(&base).send() {
                    Ok(resp) => println!("{}", resp.text().unwrap_or_default()),
                    Err(_) => println!("server not running."),
                },
            }
            Ok(())
        }
        cli::Commands::Health => {
            let cfg = Config::load(Some(&cli.config))?;
            let url = format!("http://{}/healthz", cfg.listen);
            match reqwest::blocking::get(&url) {
                Ok(resp) if resp.status().is_success() => {
                    println!("server is healthy (healthz: {})", resp.status())
                }
                Ok(resp) => println!(
                    "server is running but unhealthy (healthz: {})",
                    resp.status()
                ),
                Err(_) => println!(
                    "server is not running on {}. Start with 'portail serve'.",
                    cfg.listen
                ),
            }
            Ok(())
        }
        cli::Commands::Complexity {
            format,
            output,
            ci,
            ci_report_path,
        } => {
            let ci = *ci;
            let dir = std::path::Path::new("src");
            let report = match complexity::analyze_directory(dir) {
                Ok(r) => r,
                Err(e) => {
                    if ci {
                        // CI mode: write error report to file, always exit 0
                        let err_report = format!("error: {}", e);
                        std::fs::write(ci_report_path, &err_report).ok();
                        println!("{}", err_report);
                        return Ok(());
                    }
                    return Err(e);
                }
            };
            let report_text = complexity::generate_report(&report);

            // CI mode: write TOML report to file, always exit 0
            // Daily-only: skip if report is from today
            if ci {
                let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                if ci_report_path.exists() {
                    if let Ok(meta) = std::fs::metadata(ci_report_path) {
                        if let Ok(modified) = meta.modified() {
                            let modified_date = chrono::DateTime::<chrono::Utc>::from(modified)
                                .format("%Y-%m-%d")
                                .to_string();
                            if modified_date == today {
                                println!(
                                    "complexity-ci: report already generated today ({}) — skipping",
                                    today
                                );
                                return Ok(());
                            }
                        }
                    }
                }
                let toml_report = toml::to_string_pretty(&report).unwrap_or_default();
                std::fs::write(
                    ci_report_path,
                    format!("# generated: {}\n{}", today, toml_report),
                )
                .ok();
                // Print summary to stdout for CI log
                println!(
                    "complexity-ci: wrote report to {}",
                    ci_report_path.display()
                );
                println!(
                    "  files: {} | functions: {} | annotations: {}",
                    report.files_scanned, report.total_functions, report.total_annotations
                );
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
                std::process::Command::new("open")
                    .arg("target/doc/portail/index.html")
                    .spawn()?;
                #[cfg(target_os = "linux")]
                std::process::Command::new("xdg-open")
                    .arg("target/doc/portail/index.html")
                    .spawn()?;
            }
            Ok(())
        }
        cli::Commands::Setup {
            non_interactive: _,
            domain,
            self_signed,
            headscale,
        } => {
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
        cli::Commands::Doctor => {
            cli::doctor::run_doctor()?;
            Ok(())
        }
        cli::Commands::Pit { scan } => {
            let config = portail_agents::pit::PitConfig::default();
            if *scan {
                let pit = portail_agents::pit::Pit::new(config)?;
                let n = pit.scan_and_record();
                println!("PIT scan: {n} new processes");
                if n > 0 {
                    println!("  PIT:   {}", pit.pit_path().display());
                    println!("  Log:   {}", pit.log_path().display());
                }
            } else {
                portail_agents::pit::run_pit_watcher(config).await;
            }
            Ok(())
        }
        cli::Commands::Target { action } => {
            match action {
                cli::TargetAction::List => {
                    let cfg = portail::config::Config::load(Some(&cli.config))?;
                    let targets = if cfg.targets.is_empty() {
                        portail::config::builtin_targets()
                    } else {
                        cfg.targets
                    };
                    println!("Target templates ({} total):", targets.len());
                    for t in &targets {
                        let tag_str = t.tags.join(",");
                        println!(
                            "  {:<20} {:<12} {:3}/s  [{}]",
                            t.name,
                            format!("{}/", t.provider),
                            t.rps,
                            tag_str
                        );
                    }
                }
                cli::TargetAction::Export { name } => {
                    let cfg = portail::config::Config::load(Some(&cli.config))?;
                    let builtins = portail::config::builtin_targets();
                    let all: Vec<_> = cfg
                        .targets
                        .iter()
                        .chain(builtins.iter())
                        .filter(|t| t.name == *name)
                        .collect();
                    match all.first() {
                        Some(t) => println!("{}", serde_json::to_string_pretty(t)?),
                        None => println!("Target '{name}' not found"),
                    }
                }
                cli::TargetAction::Builtins => {
                    for t in portail::config::builtin_targets() {
                        println!(
                            "{} ({}) — {} — models: {}",
                            t.name,
                            t.provider,
                            t.description.as_deref().unwrap_or(""),
                            t.models.join(", ")
                        );
                    }
                }
            }
            Ok(())
        }
        cli::Commands::Mcp { action } => {
            match action {
                cli::McpAction::List => {
                    let cfg = portail::config::Config::load(Some(&cli.config))?;
                    let servers = cfg
                        .mcp
                        .as_ref()
                        .and_then(|m| m.server_registry.as_ref())
                        .map(|s| s.as_slice())
                        .unwrap_or(&[]);
                    if servers.is_empty() {
                        println!("MCP servers: (none configured)");
                        println!("  Built-in templates: portail mcp builtins");
                    } else {
                        println!("MCP servers ({} total):", servers.len());
                        for s in servers {
                            let tag_str = s.tags.join(",");
                            let transport = &s.transport;
                            println!(
                                "  {:<20} {:<8} auto={}  [{}]",
                                s.name, transport, s.autostart, tag_str
                            );
                        }
                    }
                }
                cli::McpAction::Info { name } => {
                    let cfg = portail::config::Config::load(Some(&cli.config))?;
                    let builtins = portail::config::builtin_mcp_servers();
                    let all: Vec<_> = cfg
                        .mcp
                        .as_ref()
                        .and_then(|m| m.server_registry.as_ref())
                        .into_iter()
                        .flatten()
                        .chain(builtins.iter())
                        .filter(|s| s.name == *name)
                        .collect();
                    match all.first() {
                        Some(s) => {
                            println!("MCP Server: {}", s.name);
                            println!("  Transport: {}", s.transport);
                            println!("  Autostart: {}", s.autostart);
                            println!("  Description: {}", s.description.as_deref().unwrap_or("-"));
                            if let Some(cmd) = &s.command {
                                let args = s.args.as_ref().map(|a| a.join(" ")).unwrap_or_default();
                                println!("  Command: {} {}", cmd, args);
                            }
                            if let Some(url) = &s.url {
                                println!("  URL: {}", url);
                            }
                            println!("  Tags: {}", s.tags.join(", "));
                        }
                        None => println!("MCP server '{name}' not found"),
                    }
                }
                cli::McpAction::Config { name } => {
                    let s = portail::config::builtin_mcp_servers()
                        .into_iter()
                        .find(|s| s.name == *name);
                    match s {
                        Some(s) => {
                            println!(
                                "# MCP server config for: {} ({})",
                                s.name,
                                s.description.unwrap_or_default()
                            );
                            println!("[[mcp.server_registry]]");
                            println!("name = \"{}\"", s.name);
                            println!("transport = \"{}\"", s.transport);
                            if let Some(cmd) = &s.command {
                                println!("command = \"{}\"", cmd);
                            }
                            if let Some(args) = &s.args {
                                println!(
                                    "args = {}",
                                    serde_json::to_string_pretty(args).unwrap_or_default()
                                );
                            }
                            println!("autostart = true");
                        }
                        None => println!(
                            "Built-in MCP server '{name}' not found. Available: filesystem, github, playwright, fetch, brave-search, sqlite, sequential-thinking"
                        ),
                    }
                }
                cli::McpAction::Builtins => {
                    for s in portail::config::builtin_mcp_servers() {
                        println!(
                            "  {:<20} {}",
                            s.name,
                            s.description.as_deref().unwrap_or("")
                        );
                    }
                }
            }
            Ok(())
        }
        cli::Commands::Vaked { action } => {
            match action {
                cli::VakedAction::List => {
                    let mut registry =
                        portail_vaked::PluginRegistry::new(std::path::PathBuf::from("vaked"));
                    registry.scan_dir().ok();
                    println!("{}", portail_vaked::format_plugin_list(&registry));
                }
                cli::VakedAction::Load { path } => {
                    let mut registry = portail_vaked::PluginRegistry::new(
                        path.parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_path_buf(),
                    );
                    match registry.load_vaked(path) {
                        Ok(name) => println!("Loaded .vaked plugin: {name}"),
                        Err(e) => println!("Error: {e}"),
                    }
                }
                cli::VakedAction::Lower { path } => {
                    let raw = std::fs::read_to_string(path)?;
                    let vaked = portail_plugin_sdk::VakedFile::from_toml(&raw)?;
                    let nix = vaked.lower_to_nix();
                    println!("{nix}");
                }
                cli::VakedAction::Build { path } => {
                    portail_vaked::build_vaked(path)?;
                }
            }
            Ok(())
        }
        cli::Commands::ReleaseAudit { dir, version, out } => {
            let out_dir = out.clone().unwrap_or_else(|| dir.join("audit"));
            portail::release_audit::run_pipeline(dir, version, &out_dir)?;
            Ok(())
        }
        cli::Commands::Dev { action } => {
            cli::dev::run_dev(action)?;
            Ok(())
        }
        cli::Commands::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = cli::Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(*shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
        cli::Commands::Loop { action } => dispatch_loop(action, cli).await,
        cli::Commands::PkgCtx { action } => dispatch_pkg_ctx(action).await,
        cli::Commands::Serve => unreachable!(),
    }
}

fn create_engine() -> loopeng::LoopEngine {
    let mut engine = loopeng::LoopEngine::new(loopeng::LoopEngineConfig {
        name: "portail".into(),
        token_budget: Some(100_000),
        escalate_after_failures: 3,
        circuit_breaker_threshold: 5,
        ..Default::default()
    });
    engine.add_schedule(loopeng::Schedule {
        name: "portail-dev".into(),
        cadence_secs: 3600,
        pattern: "dev-loop".into(),
        max_iterations: Some(20),
        enabled: true,
    });
    engine.add_schedule(loopeng::Schedule {
        name: "release".into(),
        cadence_secs: 86400,
        pattern: "release-loop".into(),
        max_iterations: Some(5),
        enabled: true,
    });
    engine
}

async fn dispatch_loop(action: &cli::LoopAction, _cli: &cli::Cli) -> anyhow::Result<()> {
    match action {
        cli::LoopAction::Status => {
            println!(
                "Loop state — use 'portail loop run <schedule>' to execute, 'portail loop prompt' for handoff"
            );
        }
        cli::LoopAction::Backlog => {
            println!("Backlog — use loop-state-manager: portail loop add <phase> <description>");
        }
        cli::LoopAction::Add { phase, description } => {
            println!("Added task [{phase}]: {description} — use via loop-state-manager MCP tools");
        }
        cli::LoopAction::Approve { task_id, reason } => {
            println!("Approved {task_id} (reason: {:?})", reason);
        }
        cli::LoopAction::Reject { task_id, reason } => {
            println!("Rejected {task_id}: {reason}");
        }
        cli::LoopAction::Next { phase } => {
            println!(
                "Next task for phase {:?} — use via loop-state-manager",
                phase
            );
        }
        cli::LoopAction::Prompt => {
            let engine = create_engine();
            let prompt = engine.generate_next_prompt("portail");
            let path = std::path::Path::new("_next-prompt.md");
            prompt.write_to_file(path)?;
            println!("Written: {}", path.display());
            println!("{}", prompt.to_prompt());
        }
        cli::LoopAction::History { .. } => {
            println!("History — run iterations first, then check prompt or status");
        }
        cli::LoopAction::Run { schedule, count } => {
            let mut engine = create_engine();
            for i in 0..*count {
                match engine.run_iteration(schedule).await {
                    Ok(run) => {
                        let status_str = format!("{:?}", run.status);
                        println!(
                            "[{}/{}] {} — {} ({})",
                            i + 1,
                            count,
                            run.id,
                            status_str,
                            run.token_cost
                                .map(|c| format!("{} tokens", c))
                                .unwrap_or_default()
                        );
                    }
                    Err(e) => {
                        eprintln!("[{}/{}] Error: {e}", i + 1, count);
                        break;
                    }
                }
            }
            let prompt = engine.generate_next_prompt(schedule);
            let path = std::path::Path::new("_next-prompt.md");
            let _ = prompt.write_to_file(path);
        }
        cli::LoopAction::Council {
            run_id,
            decision,
            reason,
        } => {
            let mut engine = create_engine();
            let council = match decision.to_lowercase().as_str() {
                "ship" => loopeng::CouncilDecision::Ship,
                "iterate" => loopeng::CouncilDecision::Iterate {
                    reason: reason.clone().unwrap_or_else(|| "Manual iteration".into()),
                },
                "escalate" => loopeng::CouncilDecision::Escalate {
                    reason: reason.clone().unwrap_or_else(|| "Manual escalation".into()),
                    context: "CLI council override".into(),
                },
                other => {
                    eprintln!("Unknown decision '{other}'. Use: ship, iterate, or escalate");
                    return Ok(());
                }
            };
            match engine.override_decision(run_id, council) {
                Ok(()) => println!("Council decision applied to {run_id}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        cli::LoopAction::ResetCircuit => {
            let mut engine = create_engine();
            engine.reset_circuit_breaker();
            println!("Circuit breaker reset");
        }
        cli::LoopAction::Config => {
            let engine = create_engine();
            let cfg = engine.config();
            println!("Loop Engine Config:");
            println!("  Name: {}", cfg.name);
            println!("  Max iterations: {}", cfg.max_iterations);
            println!("  Token budget: {:?}", cfg.token_budget);
            println!("  Escalate after: {} failures", cfg.escalate_after_failures);
            println!(
                "  Circuit breaker threshold: {}",
                cfg.circuit_breaker_threshold
            );
            println!("  Evaluation criteria: {:?}", cfg.evaluation_criteria);
            println!("\nBuilding blocks:");
            println!("  Schedules: {}", engine.schedules().len());
            println!("  Skills: {}", engine.skills().len());
            println!("  Plugins: {}", engine.plugins().len());
            println!("  Sub-agents: {}", engine.sub_agents().len());
            println!("  Memory entries: {}", engine.memory_entries().len());
        }
        cli::LoopAction::Schedules => {
            let engine = create_engine();
            for s in engine.schedules() {
                println!(
                    "  {} — every {}s (pattern: {}, enabled: {}, max_iter: {:?})",
                    s.name, s.cadence_secs, s.pattern, s.enabled, s.max_iterations
                );
            }
        }
    }
    Ok(())
}

async fn dispatch_pkg_ctx(action: &cli::PkgCtxAction) -> anyhow::Result<()> {
    use pkg_ctx::{PKG_DIR, build, mcp_server, search};
    use std::path::Path;

    let pkg_dir = dirs::data_dir()
        .map(|d| d.join("portail").join(PKG_DIR))
        .unwrap_or_else(|| Path::new(PKG_DIR).to_path_buf());
    std::fs::create_dir_all(&pkg_dir)?;

    match action {
        cli::PkgCtxAction::Add {
            repo,
            name,
            version,
        } => {
            println!("Adding package from {repo}...");
            let info =
                build::add_package(repo, name.as_deref(), version.as_deref(), &pkg_dir).await?;
            println!("  Package: {}@{}", info.name, info.version);
            println!("  Chunks: {}", info.chunk_count);
            println!("  DB: {}", info.db_path.display());
        }
        cli::PkgCtxAction::Search { library, topic } => {
            let searcher = search::DocSearch::new(&pkg_dir);
            let results = searcher.search_package(library, topic, 10)?;
            println!(
                "{}",
                search::format_search_results(&results, library, topic)
            );
        }
        cli::PkgCtxAction::List => {
            let searcher = search::DocSearch::new(&pkg_dir);
            let packages = searcher.list_installed()?;
            if packages.is_empty() {
                println!("No packages installed. Use `portail pkg-ctx add <repo>` to add one.");
            } else {
                println!("Installed packages:");
                for pkg in packages {
                    println!("  {pkg}");
                }
            }
        }
        cli::PkgCtxAction::Serve => {
            println!("Starting pkg-ctx MCP server (stdio)...");
            mcp_server::serve_stdio(Some(&pkg_dir)).await?;
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = sigterm.recv() => tracing::info!("SIGTERM received"),
    }
}
