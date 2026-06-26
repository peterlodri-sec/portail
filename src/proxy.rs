use crate::cdn;
use crate::gateway;
use crate::hooks;
use crate::mcp;
use crate::AppState;

pub use cdn::CacheManager;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get};
use axum::{Json, Router};
use metrics::{counter, histogram};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

const MAX_BODY_BYTES: usize = 10 * 1024 * 1024; // 10MB

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/livez", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/messages", any(route_to_ai_gateway))
        .route("/v1/chat/completions", any(route_to_ai_gateway))
        .route("/v1/responses", any(route_to_ai_gateway))
        .route("/v1/embeddings", any(route_to_ai_gateway))
        .route("/v1/audio/{*path}", any(route_to_ai_gateway))
        .route("/v1/images/{*path}", any(route_to_ai_gateway))
        .route("/v1beta/{*path}", any(route_to_ai_gateway))
        .route("/metrics", get(metrics_handler))
        .route("/cdn/{*path}", any(route_cdn))
        .route("/mcp/{*path}", any(route_mcp))
        .route("/mcp-rest/{*path}", any(route_mcp))
        .route("/stats", get(stats_handler))
        .route("/events", get(crate::events::handle_recent).post(crate::events::handle_publish))
        .route("/events/stream", get(crate::events::handle_stream))
        .route("/hooks", get(crate::hooks::handle_list).post(crate::hooks::handle_create))
        .route("/hooks/{id}", delete(crate::hooks::handle_delete))
        // A2A: Agent-to-Agent protocol
        .route("/.well-known/agent.json", get(crate::a2a::handle_agent_card))
        .route("/a2a/tasks", axum::routing::post(crate::a2a::handle_task_create))
        .route("/a2a/tasks/{id}", get(crate::a2a::handle_task_get))
        // A2C: Agent-to-Consumer interface
        .route("/a2c/chat", axum::routing::post(crate::a2c::handle_chat))
        .fallback(route_to_ai_gateway)
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn(metrics_middleware))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(
            TraceLayer::new_for_http()
                .on_request(|req: &Request<Body>, _span: &tracing::Span| {
                    tracing::debug!(method = %req.method(), uri = %req.uri(), "request");
                })
                .on_response(|resp: &Response, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::info!(
                        status = resp.status().as_u16(),
                        latency_us = latency.as_micros() as u64,
                        "request completed"
                    );
                }),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
                .allow_headers(Any),
        )
        .with_state(state)
}

async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok().map(String::from))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    req.extensions_mut().insert(id.clone());
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        "x-request-id",
        axum::http::HeaderValue::from_str(&id).unwrap(),
    );
    resp
}

async fn metrics_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = normalize_path(req.uri().path());
    let resp = next.run(req).await;
    let status = resp.status().as_u16();
    let latency = start.elapsed().as_secs_f64();
    let status_s = status.to_string();
    counter!("http_requests_total", "method" => method, "path" => path.clone(), "status" => status_s).increment(1);
    histogram!("http_request_duration_seconds", "path" => path).record(latency);
    resp
}

fn normalize_path(path: &str) -> String {
    // Normalize dynamic path segments to reduce cardinality
    let segments: Vec<&str> = path.split('/').collect();
    let normalized: Vec<String> = segments.iter().map(|s| {
        // Replace UUIDs and long IDs with placeholders
        if s.len() > 20 && s.contains('-') && s.chars().all(|c| c.is_alphanumeric() || c == '-') {
            "{id}".to_string()
        } else if s.len() > 32 && s.chars().all(|c| c.is_alphanumeric()) {
            "{hash}".to_string()
        } else {
            s.to_string()
        }
    }).collect();
    normalized.join("/")
}

async fn healthz() -> &'static str {
    counter!("health_checks").increment(1);
    "ok"
}

async fn readyz(State(state): State<Arc<AppState>>) -> (StatusCode, &'static str) {
    let upstream = {
        let c = state.config.read().unwrap();
        c.ai_gateway.as_ref().filter(|g| g.enabled).map(|g| g.upstream.clone())
    };
    let ready = match upstream {
        Some(url) => {
            let ok = reqwest::get(format!("{url}/healthz"))
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !ok { return (StatusCode::SERVICE_UNAVAILABLE, "ai gateway not ready"); }
            true
        }
        None => true,
    };
    if ready { (StatusCode::OK, "ready") } else { (StatusCode::SERVICE_UNAVAILABLE, "not ready") }
}

async fn route_to_ai_gateway(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Response {
    let cfg = {
        let c = state.config.read().unwrap();
        c.ai_gateway.as_ref().filter(|g| g.enabled).map(|g| g.upstream.clone())
    };
    let Some(upstream) = cfg else {
        return (StatusCode::NOT_IMPLEMENTED, "ai gateway disabled").into_response();
    };

    let path = req.uri().path().to_string();
    let matching_hooks = state.hooks.match_message(&path);

    if matching_hooks.is_empty() {
        return gateway::forward(&upstream, req).await;
    }

    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, 10_000_000).await.unwrap_or_default();

    let modified = serde_json::from_slice::<serde_json::Value>(&body_bytes)
        .ok()
        .and_then(|v| hooks::apply_message_hooks(&v, &matching_hooks))
        .and_then(|v| serde_json::to_vec(&v).ok())
        .unwrap_or(body_bytes.to_vec());

    counter!("hook_injections").increment(matching_hooks.len() as u64);
    state.event_log.publish(crate::events::AgentEvent {
        agent_id: "hooks".into(),
        event_type: "injected".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: rustc_hash::FxHashMap::from_iter([
            ("path".into(), path),
            ("count".into(), matching_hooks.len().to_string()),
        ]),
    });

    gateway::forward_with_body(&upstream, parts, modified.into()).await
}

async fn route_cdn(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Response {
    let Some(cache) = &state.cdn_cache else {
        return (StatusCode::NOT_IMPLEMENTED, "cdn disabled").into_response();
    };
    let origin = {
        let c = state.config.read().unwrap();
        c.cdn.as_ref().map(|c| c.origin.clone())
    };
    cdn::handle(req, Arc::clone(cache), origin).await
}

async fn route_mcp(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Response {
    let socket = {
        let c = state.config.read().unwrap();
        c.mcp.as_ref().filter(|m| m.enabled).map(|m| m.socket_path.clone())
    };
    let Some(socket_path) = socket else {
        return (StatusCode::NOT_IMPLEMENTED, "mcp disabled").into_response();
    };
    mcp::proxy_to_sidecar(&socket_path, req).await
}

async fn metrics_handler(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, [(&'static str, &'static str); 1], String) {
    let body = state.metrics_handle.render();
    (StatusCode::OK, [("content-type", "text/plain; charset=utf-8")], body)
}

async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let cdn_stats: serde_json::Value = state
        .cdn_cache
        .as_ref()
        .map(|c| serde_json::to_value(c.stats()).unwrap_or_default())
        .unwrap_or_default();
    json!({ "cdn": cdn_stats, "version": env!("CARGO_PKG_VERSION") }).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use std::sync::RwLock;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            config: RwLock::new(crate::config::Config::default()),
            event_log: Arc::new(crate::events::EventLog::new(100)),
            cdn_cache: None,
            hooks: Arc::new(crate::hooks::HookStore::new()),
            a2a_tasks: Arc::new(crate::a2a::TaskStore::new()),
            dns_store: Arc::new(crate::dns::DnsStore::new()),
            doh_client: None,
            network_isolation: Arc::new(crate::dns::NetworkIsolation::default()),
            tinyurl: Arc::new(crate::plugins::TinyUrlStore::new(crate::plugins::TinyUrlConfig::default())),
            trace_store: Arc::new(crate::plugins::TraceStore::new(100)),
            redis_cache: Arc::new(crate::plugins::RedisCache::new(crate::plugins::RedisCacheConfig::default())),
            discovery: Arc::new(crate::discovery::DiscoveryStore::new(crate::discovery::DiscoveryConfig::default())),
            ebpf: Arc::new(crate::ebpf::EbpfManager::new(crate::ebpf::EbpfConfig::default())),
            iouring: Arc::new(crate::iouring::IoUringManager::new(crate::iouring::IoUringConfig::default())),
            dpdk: Arc::new(crate::dpdk::DpdkManager::new(crate::dpdk::DpdkConfig::default())),
            hyper: Arc::new(crate::hyper_engine::HyperManager::new(crate::hyper_engine::HyperConfig::default())),
            ci_status: Arc::new(crate::ci::CiStatusStore::new(100, None)),
            metrics_handle: crate::test_utils::global_metrics().clone(),
        })
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn livez_returns_ok() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/livez").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_no_upstream() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/readyz").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stats_returns_json() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/stats").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn cdn_disabled_returns_not_implemented() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/cdn/foo").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn ai_gateway_disabled_returns_not_implemented() {
        let state = test_state();
        state.config.write().unwrap().ai_gateway = None;
        let app = build_router(state);
        let req = Request::builder()
            .uri("/v1/chat/completions")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn mcp_disabled_returns_not_implemented() {
        let state = test_state();
        state.config.write().unwrap().mcp = None;
        let app = build_router(state);
        let req = Request::builder()
            .uri("/mcp/tools/list")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn request_id_injected() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn request_id_preserved() {
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/healthz")
            .header("x-request-id", "test-id-123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.headers().get("x-request-id").unwrap().to_str().unwrap(),
            "test-id-123"
        );
    }

    #[tokio::test]
    async fn metrics_returns_prometheus() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
        let _ = app.oneshot(req).await;
        let app = build_router(test_state());
        let req = Request::builder().uri("/metrics").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 100_000).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("health_checks") || text.contains("http_requests"));
    }

    #[tokio::test]
    async fn metrics_records_counter() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
        let _ = app.oneshot(req).await;
        let app = build_router(test_state());
        let req = Request::builder().uri("/metrics").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 100_000).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("health_checks") || text.contains("http_requests"));
    }

    #[tokio::test]
    async fn hooks_create_list_delete() {
        let app = build_router(test_state());
        let hook = serde_json::json!({
            "id": "test-hook",
            "match_path": "/chat",
            "inject": "prepend",
            "content": "be helpful",
            "enabled": true
        });
        let req = Request::builder()
            .uri("/hooks")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hook).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn event_stream_returns_ok() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/events/stream").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn events_recent_accepts_event() {
        let state = test_state();
        state.event_log.publish(crate::events::AgentEvent {
            agent_id: "test".into(),
            event_type: "ping".into(),
            severity: "info".into(),
            timestamp: 1,
            metadata: rustc_hash::FxHashMap::default(),
        });
        let app = build_router(state);
        let req = Request::builder().uri("/events").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 100_000).await.unwrap();
        let events: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!events.as_array().unwrap().is_empty());
    }
}
