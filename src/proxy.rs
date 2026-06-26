use crate::cdn;
use crate::gateway;
use crate::mcp;
use crate::AppState;

pub use cdn::CacheManager;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use metrics::{counter, histogram};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

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
        .fallback(route_to_ai_gateway)
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn(metrics_middleware))
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
        .layer(CorsLayer::permissive())
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
    let path = req.uri().path().to_string();
    let resp = next.run(req).await;
    let status = resp.status().as_u16();
    let latency = start.elapsed().as_secs_f64();
    let status_s = status.to_string();
    counter!("http_requests_total", "method" => method, "path" => path.clone(), "status" => status_s).increment(1);
    histogram!("http_request_duration_seconds", "path" => path).record(latency);
    resp
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
    gateway::forward(&upstream, req).await
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
    use std::sync::OnceLock;
    use tower::ServiceExt;

    fn global_metrics() -> &'static metrics_exporter_prometheus::PrometheusHandle {
        static HANDLE: OnceLock<metrics_exporter_prometheus::PrometheusHandle> = OnceLock::new();
        HANDLE.get_or_init(|| {
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .install_recorder()
                .expect("install metrics recorder")
        })
    }

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            config: RwLock::new(crate::config::Config::default()),
            cdn_cache: None,
            metrics_handle: global_metrics().clone(),
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
        // hit healthz to trigger counter
        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
        let _ = app.oneshot(req).await;
        // now check /metrics includes the counter
        let app = build_router(test_state());
        let req = Request::builder().uri("/metrics").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 100_000).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("health_checks") || text.contains("http_requests"));
    }
}
