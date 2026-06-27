//! Provider Handlers — each provider's full request lifecycle in one place.
//!
//! A handler owns the complete flow for one provider:
//!
//!   1. Feature virtualization (apply fallbacks for unsupported features)
//!   2. Request body adaptation (OpenAI format → provider format)
//!   3. URI path rewriting (e.g. /v1/chat/completions → /api/chat)
//!   4. Provider-specific header injection (e.g. anthropic-version)
//!   5. HTTP forwarding to upstream
//!   6. Response body adaptation (provider format → OpenAI format)
//!
//! # Adding a new provider
//!
//! 1. Create a struct for your provider
//! 2. Implement `ProviderHandler` for it
//! 3. Register it in `registry()`
//! 4. Add a `provider_path()` entry in `target_router.rs` if the API endpoint differs

use super::features::{self, Support};
use super::schema::{self, ProviderAdapter};
use crate::target_router;
use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use metrics::counter;
use std::sync::OnceLock;
use tracing::{debug, warn};

// ── Types shared across handlers ──────────────────────────────────────

const HOP_BY_HOP: &[&str] = &[
    "host",
    "connection",
    "transfer-encoding",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
];

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .http2_keep_alive_interval(Some(std::time::Duration::from_secs(30)))
            .build()
            .expect("failed to build HTTP client")
    })
}

fn strip_hop_by_hop(headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (key, value) in headers.iter() {
        if !HOP_BY_HOP.contains(&key.as_str()) {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

fn add_x_forwarded_for(headers: &mut HeaderMap) {
    match headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        Some(existing) => {
            headers.insert(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_str(&format!("{existing}, portail")).unwrap(),
            );
        }
        None => {
            headers.insert(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static("portail"),
            );
        }
    }
}

// ── Trait ────────────────────────────────────────────────────────────

/// A provider handler owns the full request → response lifecycle for one
/// AI provider. Everything from body transformation to path rewriting to
/// response adaptation lives in one place.
#[async_trait]
pub trait ProviderHandler: Send + Sync {
    /// Provider name, e.g. "openai", "anthropic", "ollama"
    fn name(&self) -> &'static str;

    /// Full request lifecycle for this provider.
    async fn handle(
        &self,
        upstream: &str,
        parts: axum::http::request::Parts,
        body_bytes: Bytes,
    ) -> Response;

    /// Capability matrix for this provider — what features it supports natively.
    fn capabilities(&self) -> Vec<(&'static str, Support)> {
        features::capabilities(self.name())
    }

    /// Build provider-specific extra headers (e.g. anthropic-version).
    fn extra_headers(&self) -> Vec<(&'static str, String)> {
        vec![]
    }

    /// Rewrite the request URI path if the provider uses a different endpoint.
    fn rewrite_path(&self, path: &str) -> String {
        target_router::provider_path(self.name(), path)
    }
}

// ── Registry ─────────────────────────────────────────────────────────

static REGISTRY: OnceLock<Vec<Box<dyn ProviderHandler>>> = OnceLock::new();

pub fn registry() -> &'static [Box<dyn ProviderHandler>] {
    REGISTRY.get_or_init(|| {
        vec![
            Box::new(OpenAiHandler),
            Box::new(DeepSeekHandler),
            Box::new(AnthropicHandler),
            Box::new(GoogleHandler),
            Box::new(OllamaHandler),
        ]
    })
}

pub fn by_name(name: &str) -> &'static dyn ProviderHandler {
    registry()
        .iter()
        .find(|h| h.name() == name)
        .map(|h| h.as_ref())
        .unwrap_or(&OpenAiHandler)
}

// ── Core forward function (shared by all handlers) ────────────────────

/// Shared HTTP forwarding: strips hop-by-hop headers, injects provider
/// headers, sends the request, returns decomposed response.
async fn forward_raw(
    upstream: &str,
    parts: axum::http::request::Parts,
    body_bytes: Bytes,
    extra_headers: HeaderMap,
) -> Response {
    let uri = &parts.uri;
    let method = parts.method.clone();
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let upstream_url = format!("{}{}", upstream.trim_end_matches('/'), path);

    let mut forward_headers = strip_hop_by_hop(&parts.headers);
    add_x_forwarded_for(&mut forward_headers);
    forward_headers.remove("content-length");
    for (key, value) in extra_headers.iter() {
        forward_headers.insert(key.clone(), value.clone());
    }

    debug!(method = %method, %upstream_url, body_size = body_bytes.len(), "handler_forward");

    let req_builder = match method {
        axum::http::Method::GET => client().get(&upstream_url),
        axum::http::Method::POST => client().post(&upstream_url),
        axum::http::Method::PUT => client().put(&upstream_url),
        axum::http::Method::DELETE => client().delete(&upstream_url),
        axum::http::Method::PATCH => client().patch(&upstream_url),
        _ => client().get(&upstream_url),
    }
    .headers(forward_headers)
    .body(body_bytes);

    match req_builder.send().await {
        Ok(resp) => {
            counter!("ai_gateway_requests", "status" => resp.status().as_u16().to_string())
                .increment(1);
            let status = resp.status();
            let mut out_headers = strip_hop_by_hop(resp.headers());
            out_headers.insert("x-portail-proxy", HeaderValue::from_static("ai-gateway"));
            let body = resp.bytes().await.unwrap_or_default();
            (status, out_headers, body).into_response()
        }
        Err(e) => {
            warn!(%upstream_url, error = %e, "upstream unreachable");
            counter!("ai_gateway_errors").increment(1);
            let mut err_headers = HeaderMap::new();
            err_headers.insert("content-type", HeaderValue::from_static("text/plain"));
            (StatusCode::BAD_GATEWAY, err_headers, "upstream unavailable").into_response()
        }
    }
}

/// Build a response with body adaptation, stripping stale content-length.
fn build_adapted_response(status: StatusCode, headers: &HeaderMap, body: Vec<u8>) -> Response {
    let mut out = HeaderMap::new();
    for (k, v) in headers.iter() {
        let key = k.as_str().to_lowercase();
        if key != "content-length" && key != "transfer-encoding" {
            out.insert(k.clone(), v.clone());
        }
    }
    out.insert("x-portail-proxy", HeaderValue::from_static("portail-ai"));
    (status, out, body).into_response()
}

// ── Helpers ──────────────────────────────────────────────────────────

fn build_headers(extra: Vec<(&'static str, String)>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in extra {
        if let Ok(hv) = HeaderValue::from_str(&value) {
            headers.insert(HeaderName::from_static(name), hv);
        }
    }
    headers
}

/// Apply request body adaptation + default params injection.
fn adapt_request_body(body_bytes: Bytes, adapter: &dyn ProviderAdapter) -> Bytes {
    if let Ok(mut body_val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        let defaults = adapter.default_params();
        if let Some(map) = body_val.as_object_mut() {
            for (k, v) in defaults.as_object().unwrap_or(&serde_json::Map::new()) {
                map.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        if let Err(e) = adapter.adapt_request(&mut body_val) {
            warn!(error = %e, provider = %adapter.name(), "request adaptation failed");
            body_bytes
        } else if let Ok(adapted) = serde_json::to_vec(&body_val) {
            adapted.into()
        } else {
            body_bytes
        }
    } else {
        body_bytes
    }
}

/// Adapt a non-streaming JSON response body from provider → OpenAI format.
fn adapt_response_body(resp: &mut Response, adapter: &dyn ProviderAdapter) -> Option<Vec<u8>> {
    let body = std::mem::take(resp.body_mut());
    let body_bytes = futures::executor::block_on(axum::body::to_bytes(body, 10_000_000)).ok()?;
    if let Ok(mut val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        if adapter.adapt_response(&mut val).is_ok() {
            Some(serde_json::to_vec(&val).unwrap_or(body_bytes.to_vec()))
        } else {
            None
        }
    } else {
        None
    }
}

fn is_streaming_response(resp: &Response) -> bool {
    resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream") || ct.contains("text/plain"))
        .unwrap_or(false)
}

fn is_request_streaming(body: &[u8]) -> bool {
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(stream) = val.get("stream") {
            return stream == true || stream.as_str() == Some("true");
        }
    }
    false
}

// ── OpenAI Handler (canonical baseline) ──────────────────────────────

struct OpenAiHandler;

#[async_trait]
impl ProviderHandler for OpenAiHandler {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn handle(
        &self,
        upstream: &str,
        parts: axum::http::request::Parts,
        body_bytes: Bytes,
    ) -> Response {
        // OpenAI is the canonical format — just forward
        forward_raw(upstream, parts, body_bytes, HeaderMap::new()).await
    }
}

// ── DeepSeek Handler (OpenAI-compatible, minor response tweaks) ─────

struct DeepSeekHandler;

#[async_trait]
impl ProviderHandler for DeepSeekHandler {
    fn name(&self) -> &'static str {
        "deepseek"
    }

    async fn handle(
        &self,
        upstream: &str,
        parts: axum::http::request::Parts,
        body_bytes: Bytes,
    ) -> Response {
        let adapter = schema::by_name("deepseek");
        let mut resp = forward_raw(upstream, parts, body_bytes, HeaderMap::new()).await;

        // Adapt response body (non-streaming only)
        if !is_streaming_response(&resp) {
            if let Some(adapted) = adapt_response_body(&mut resp, adapter) {
                build_adapted_response(resp.status(), resp.headers(), adapted)
            } else {
                resp
            }
        } else {
            resp
        }
    }
}

// ── Anthropic Handler (major schema differences) ────────────────────

struct AnthropicHandler;

#[async_trait]
impl ProviderHandler for AnthropicHandler {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn extra_headers(&self) -> Vec<(&'static str, String)> {
        vec![
            ("anthropic-version", "2023-06-01".to_string()),
            (
                "x-api-key",
                std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            ),
        ]
    }

    async fn handle(
        &self,
        upstream: &str,
        mut parts: axum::http::request::Parts,
        body_bytes: Bytes,
    ) -> Response {
        let adapter = schema::by_name("anthropic");

        // 1. Feature virtualize (handle unsupported params like frequency_penalty)
        let body_bytes = {
            let mut body_val: serde_json::Value =
                serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
            let _warnings = features::virtualize_request("anthropic", &mut body_val);
            serde_json::to_vec(&body_val)
                .unwrap_or(body_bytes.to_vec())
                .into()
        };

        // 2. Adapt request body
        let adapted_body = adapt_request_body(body_bytes, adapter);

        // 3. Rewrite path: /v1/chat/completions → /v1/messages
        let original_path = parts.uri.path().to_string();
        let new_path = self.rewrite_path(&original_path);
        if new_path != original_path {
            if let Ok(uri) = new_path.parse::<axum::http::Uri>() {
                parts.uri = uri;
            }
        }

        // 4. Provider headers
        let headers = build_headers(self.extra_headers());

        // 5. Check streaming
        let is_streaming = is_request_streaming(&adapted_body);

        // 6. Forward
        let mut resp = forward_raw(upstream, parts, adapted_body, headers).await;

        // 7. Adapt response (non-streaming only)
        if !is_streaming && !is_streaming_response(&resp) {
            if let Some(adapted) = adapt_response_body(&mut resp, adapter) {
                build_adapted_response(resp.status(), resp.headers(), adapted)
            } else {
                resp
            }
        } else {
            resp
        }
    }
}

// ── Google Gemini Handler ───────────────────────────────────────────

struct GoogleHandler;

#[async_trait]
impl ProviderHandler for GoogleHandler {
    fn name(&self) -> &'static str {
        "google"
    }

    fn extra_headers(&self) -> Vec<(&'static str, String)> {
        vec![(
            "x-goog-api-key",
            std::env::var("GOOGLE_API_KEY").unwrap_or_default(),
        )]
    }

    async fn handle(
        &self,
        upstream: &str,
        mut parts: axum::http::request::Parts,
        body_bytes: Bytes,
    ) -> Response {
        let adapter = schema::by_name("google");

        // 1. Feature virtualize
        let body_bytes = {
            let mut body_val: serde_json::Value =
                serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
            let _warnings = features::virtualize_request("google", &mut body_val);
            serde_json::to_vec(&body_val)
                .unwrap_or(body_bytes.to_vec())
                .into()
        };

        // 2. Adapt body
        let adapted_body = adapt_request_body(body_bytes, adapter);

        // 3. Rewrite path: /v1/chat/completions → :generateContent
        let original_path = parts.uri.path().to_string();
        let new_path = self.rewrite_path(&original_path);
        if new_path != original_path {
            let model =
                body_has_model(&adapted_body).unwrap_or_else(|| "gemini-2.5-flash".to_string());
            let final_path = new_path.replace("gemini-2.5-flash", &model);
            if let Ok(uri) = final_path.parse::<axum::http::Uri>() {
                parts.uri = uri;
            }
        }

        // 4. Headers + forward
        let headers = build_headers(self.extra_headers());
        let is_streaming = is_request_streaming(&adapted_body);
        let mut resp = forward_raw(upstream, parts, adapted_body, headers).await;

        // 5. Adapt response
        if !is_streaming && !is_streaming_response(&resp) {
            if let Some(adapted) = adapt_response_body(&mut resp, adapter) {
                build_adapted_response(resp.status(), resp.headers(), adapted)
            } else {
                resp
            }
        } else {
            resp
        }
    }
}

fn body_has_model(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("model")?.as_str().map(String::from))
}

// ── Ollama Handler (different endpoint + param wrapping) ────────────

struct OllamaHandler;

#[async_trait]
impl ProviderHandler for OllamaHandler {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn handle(
        &self,
        upstream: &str,
        mut parts: axum::http::request::Parts,
        body_bytes: Bytes,
    ) -> Response {
        let adapter = schema::by_name("ollama");

        // 1. Feature virtualize (tools → emulated via prompt injection)
        let body_bytes = {
            let mut body_val: serde_json::Value =
                serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
            let _warnings = features::virtualize_request("ollama", &mut body_val);
            serde_json::to_vec(&body_val)
                .unwrap_or(body_bytes.to_vec())
                .into()
        };

        // 2. Adapt request body (OpenAI messages + params → Ollama format with options wrapping)
        let adapted_body = adapt_request_body(body_bytes, adapter);

        // 3. Rewrite path: /v1/chat/completions → /api/chat
        let original_path = parts.uri.path().to_string();
        let new_path = self.rewrite_path(&original_path);
        if new_path != original_path {
            if let Ok(uri) = new_path.parse::<axum::http::Uri>() {
                parts.uri = uri;
            }
        }

        // 4. Forward
        let is_streaming = is_request_streaming(&adapted_body);
        let mut resp = forward_raw(upstream, parts, adapted_body, HeaderMap::new()).await;

        // 5. Adapt response (Ollama `message` + `eval_count` → OpenAI `choices` + `usage`)
        if is_streaming || is_streaming_response(&resp) {
            resp
        } else if let Some(adapted) = adapt_response_body(&mut resp, adapter) {
            build_adapted_response(resp.status(), resp.headers(), adapted)
        } else {
            resp
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_contains_all_handlers() {
        let names: Vec<&str> = registry().iter().map(|h| h.name()).collect();
        assert!(names.contains(&"openai"));
        assert!(names.contains(&"deepseek"));
        assert!(names.contains(&"anthropic"));
        assert!(names.contains(&"google"));
        assert!(names.contains(&"ollama"));
    }

    #[test]
    fn test_by_name_fallback() {
        assert_eq!(by_name("openai").name(), "openai");
        assert_eq!(by_name("unknown").name(), "openai"); // fallback to OpenAI
    }

    #[test]
    fn test_extra_headers_anthropic() {
        let h = AnthropicHandler;
        let headers = h.extra_headers();
        assert!(headers.iter().any(|(k, _)| *k == "anthropic-version"));
    }

    #[test]
    fn test_extra_headers_openai_empty() {
        let h = OpenAiHandler;
        assert!(h.extra_headers().is_empty());
    }

    #[test]
    fn test_capabilities_openai() {
        let h = OpenAiHandler;
        let caps = h.capabilities();
        assert!(
            caps.iter()
                .any(|(n, s)| *n == "frequency_penalty" && matches!(s, Support::Native))
        );
    }
}
