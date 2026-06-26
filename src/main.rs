use clap::Parser;
use portail::cdn;
use portail::config;
use portail::config::Config;
use portail::AppState;
use portail::mcp;
use std::sync::{Arc, RwLock};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");

    let cli = config::Cli::parse();
    let config = Config::load(&cli)?;
    let listen = config.listen.clone();
    tracing::info!(%listen, "portail starting");

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
        cdn_cache,
        metrics_handle: handle,
    });

    let app = portail::proxy::build_router(Arc::clone(&state));

    let sighup_state = Arc::clone(&state);
    let sighup_cli = cli.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sig = signal(SignalKind::hangup())
            .expect("failed to install SIGHUP handler");
        loop {
            sig.recv().await;
            match Config::load(&sighup_cli) {
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

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = sigterm.recv() => tracing::info!("SIGTERM received"),
    }
}
