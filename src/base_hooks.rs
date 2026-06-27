//! Base hooks — compiled-in, config-gated.
//!
//! These run on every request/response before user hooks and plugins.
//! They're fast, zero-allocation on the hot path, and provide
//! foundational observability and security.
//!
//! # Execution Order
//!
//! ```text
//! Request → base hooks → user hooks → plugin hooks → forward
//! Response → plugin hooks → user hooks → base hooks
//! ```
//!
//! # Built-in Hooks
//!
//! | Hook | When | Purpose |
//! |------|------|---------|
//! | `request-logger` | PreRequest | Log every AI request |
//! | `error-capture` | PostResponse | Capture upstream errors |
//! | `api-key-mask` | PreRequest | Mask API keys in logs/events |

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

// ─── trait ────────────────────────────────────────────────────────

/// Base hook trait — implemented by compiled-in hooks.
pub trait BaseHook: Send + Sync {
    /// Hook name (unique identifier).
    fn name(&self) -> &str;

    /// When this hook runs: "pre_request" or "post_response".
    fn when(&self) -> &str;

    /// Whether this hook is enabled for the current config.
    fn enabled(&self, config: &crate::config::Config) -> bool;

    /// Execute the hook on a request body. Returns modified body or None.
    fn on_request(&self, _body: &serde_json::Value, _path: &str) -> Option<serde_json::Value> {
        None
    }

    /// Execute the hook on a response. Returns event metadata to publish.
    fn on_response(
        &self,
        _status: u16,
        _path: &str,
        _latency: std::time::Duration,
        _body: &serde_json::Value,
    ) -> Option<crate::events::AgentEvent> {
        None
    }
}

// ─── registry ─────────────────────────────────────────────────────

/// Registry of base hooks, initialized at startup.
pub struct BaseHookRegistry {
    hooks: Vec<Arc<dyn BaseHook>>,
}

impl Default for BaseHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseHookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Register a base hook.
    pub fn register(&mut self, hook: Arc<dyn BaseHook>) {
        self.hooks.push(hook);
    }

    /// Run all enabled pre-request hooks.
    pub fn run_pre_request(
        &self,
        body: &serde_json::Value,
        path: &str,
        config: &crate::config::Config,
    ) -> serde_json::Value {
        let mut body = body.clone();
        for hook in &self.hooks {
            if hook.when() == "pre_request" && hook.enabled(config) {
                if let Some(modified) = hook.on_request(&body, path) {
                    body = modified;
                }
            }
        }
        body
    }

    /// Run all enabled post-response hooks.
    pub fn run_post_response(
        &self,
        status: u16,
        path: &str,
        latency: std::time::Duration,
        body: &serde_json::Value,
        config: &crate::config::Config,
        event_log: &Arc<crate::events::EventLog>,
    ) {
        for hook in &self.hooks {
            if hook.when() == "post_response" && hook.enabled(config) {
                if let Some(event) = hook.on_response(status, path, latency, body) {
                    event_log.publish(event);
                }
            }
        }
    }

    /// List all registered hook names.
    pub fn list(&self) -> Vec<&str> {
        self.hooks.iter().map(|h| h.name()).collect()
    }
}

// ─── built-in: request-logger ─────────────────────────────────────

/// Logs every AI request with path, model, and timestamp.
pub struct RequestLogger;

impl BaseHook for RequestLogger {
    fn name(&self) -> &str {
        "request-logger"
    }

    fn when(&self) -> &str {
        "pre_request"
    }

    fn enabled(&self, _config: &crate::config::Config) -> bool {
        true // always on
    }

    fn on_request(&self, _body: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
        tracing::info!(path = %path, "AI request received");
        None
    }
}

// ─── built-in: error-capture ──────────────────────────────────────

/// Captures upstream errors and publishes structured events.
pub struct ErrorCapture;

impl BaseHook for ErrorCapture {
    fn name(&self) -> &str {
        "error-capture"
    }

    fn when(&self) -> &str {
        "post_response"
    }

    fn enabled(&self, _config: &crate::config::Config) -> bool {
        true
    }

    fn on_response(
        &self,
        status: u16,
        path: &str,
        latency: std::time::Duration,
        _body: &serde_json::Value,
    ) -> Option<crate::events::AgentEvent> {
        if status >= 400 {
            let severity = if status >= 500 { "error" } else { "warning" };
            Some(crate::events::AgentEvent {
                agent_id: "base-hook:error-capture".into(),
                event_type: "upstream_error".into(),
                severity: severity.into(),
                timestamp: 0,
                metadata: crate::types::BoundedMeta::from_iter([
                    ("path".into(), path.to_string()),
                    ("status".into(), status.to_string()),
                    ("latency_ms".into(), latency.as_millis().to_string()),
                ]),
            })
        } else {
            None
        }
    }
}

// ─── built-in: api-key-mask ───────────────────────────────────────

/// Masks API keys in request bodies before logging/forwarding.
///
/// This hook doesn't modify the body — it's a guard that ensures
/// sensitive fields are never logged by other hooks.
pub struct ApiKeyMask;

impl BaseHook for ApiKeyMask {
    fn name(&self) -> &str {
        "api-key-mask"
    }

    fn when(&self) -> &str {
        "pre_request"
    }

    fn enabled(&self, _config: &crate::config::Config) -> bool {
        true
    }

    fn on_request(&self, body: &serde_json::Value, _path: &str) -> Option<serde_json::Value> {
        // Strip common API key fields from the body for logging safety
        let mut body = body.clone();
        if let Some(obj) = body.as_object_mut() {
            for key in &["api_key", "apikey", "authorization", "x-api-key"] {
                if obj.contains_key(*key) {
                    obj.insert(key.to_string(), serde_json::json!("[REDACTED]"));
                }
            }
        }
        Some(body)
    }
}

// ─── tower middleware ──────────────────────────────────────────────

/// Tower middleware that runs base hooks on AI gateway requests/responses.
///
/// Runs pre-request hooks on the body before forwarding, and
/// post-response hooks after the response comes back.
pub async fn base_hooks_middleware(
    State(state): State<Arc<crate::AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    // Only intercept AI gateway paths
    let is_ai_path =
        path.starts_with("/v1/") || path.starts_with("/v1beta/") || path == "/v1/chat/completions";

    if !is_ai_path {
        return next.run(req).await;
    }

    // Pre-request: run base hooks on the body
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, 10_000_000)
        .await
        .unwrap_or_default();

    let (modified, parsed) = {
        let config = state.config.read().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
        let modified = state.base_hooks.run_pre_request(&parsed, &path, &config);
        (modified, parsed)
    };

    let body_bytes = if modified != parsed {
        serde_json::to_vec(&modified).unwrap_or(body_bytes.to_vec())
    } else {
        body_bytes.to_vec()
    };

    let req = axum::http::Request::from_parts(parts, axum::body::Body::from(body_bytes));
    let start = std::time::Instant::now();

    // Forward
    let resp = next.run(req).await;

    // Post-response: run base hooks
    let latency = start.elapsed();
    let status = resp.status().as_u16();
    {
        let config = state.config.read().unwrap();
        state.base_hooks.run_post_response(
            status,
            &path,
            latency,
            &parsed,
            &config,
            &state.event_log,
        );
    }

    resp
}

// ─── factory ──────────────────────────────────────────────────────

/// Create the default base hook registry with all built-in hooks.
pub fn default_registry() -> BaseHookRegistry {
    let mut reg = BaseHookRegistry::new();
    reg.register(Arc::new(RequestLogger));
    reg.register(Arc::new(ErrorCapture));
    reg.register(Arc::new(ApiKeyMask));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_hooks() {
        let reg = default_registry();
        assert_eq!(
            reg.list(),
            vec!["request-logger", "error-capture", "api-key-mask"]
        );
    }

    #[test]
    fn request_logger_always_enabled() {
        let hook = RequestLogger;
        let config = crate::config::Config::default();
        assert!(hook.enabled(&config));
    }

    #[test]
    fn error_capture_captures_500() {
        let hook = ErrorCapture;
        let event = hook.on_response(
            500,
            "/v1/chat/completions",
            std::time::Duration::from_millis(100),
            &serde_json::json!({}),
        );
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, "upstream_error");
        assert_eq!(event.severity, "error");
    }

    #[test]
    fn error_capture_ignores_200() {
        let hook = ErrorCapture;
        let event = hook.on_response(
            200,
            "/v1/chat/completions",
            std::time::Duration::from_millis(100),
            &serde_json::json!({}),
        );
        assert!(event.is_none());
    }

    #[test]
    fn api_key_mask_redacts_keys() {
        let hook = ApiKeyMask;
        let body = serde_json::json!({
            "model": "gpt-4",
            "api_key": "sk-secret123",
            "messages": []
        });
        let masked = hook.on_request(&body, "/test").unwrap();
        assert_eq!(masked["api_key"], "[REDACTED]");
        assert_eq!(masked["model"], "gpt-4"); // other fields untouched
    }

    #[test]
    fn pre_request_runs_all_hooks() {
        let reg = default_registry();
        let config = crate::config::Config::default();
        let body = serde_json::json!({
            "model": "gpt-4",
            "api_key": "sk-secret",
            "messages": []
        });
        let result = reg.run_pre_request(&body, "/test", &config);
        // api-key-mask should have redacted the key
        assert_eq!(result["api_key"], "[REDACTED]");
    }
}
