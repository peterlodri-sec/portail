/*
 * Tracer Plugin — Request/Response E2E Visualization
 *
 * Architecture:
 *
 *   ┌─────────────────────────────────────────────────────────────┐
 *   │                    Tracer Flow                              │
 *   ├─────────────────────────────────────────────────────────────┤
 *   │                                                             │
 *   │   Request                                                    │
 *   │        │                                                    │
 *   │        ▼                                                    │
 *   │   ┌────────────┐     ┌────────────┐     ┌────────────┐     │
 *   │   │  Start     │────▶│  Record    │────▶│  End       │     │
 *   │   │  Timer     │     │  Spans     │     │  Timer     │     │
 *   │   └────────────┘     └────────────┘     └────────────┘     │
 *   │        │                   │                   │             │
 *   │        ▼                   ▼                   ▼             │
 *   │   ┌─────────────────────────────────────────────────────┐   │
 *   │   │              Trace Tree                              │   │
 *   │   │                                                     │   │
 *   │   │   [request] ──────────────────────────────── 120ms  │   │
 *   │   │     ├─ [hook_inject] ───────────────────── 2ms      │   │
 *   │   │     ├─ [gateway_forward] ──────────────── 95ms      │   │
 *   │   │     │     ├─ [dns_resolve] ──────────── 5ms         │   │
 *   │   │     │     ├─ [tcp_connect] ─────────── 10ms         │   │
 *   │   │     │     ├─ [tls_handshake] ──────── 15ms          │   │
 *   │   │     │     └─ [http_response] ──────── 60ms          │   │
 *   │   │     └─ [event_publish] ─────────────── 1ms          │   │
 *   │   │                                                     │   │
 *   │   └─────────────────────────────────────────────────────┘   │
 *   │                                                             │
 *   └─────────────────────────────────────────────────────────────┘
 */

use crate::types::BoundedMeta;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: String,
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub total_duration_us: u64,
    pub spans: Vec<Span>,
    pub metadata: BoundedMeta,
    pub started_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub duration_us: u64,
    pub status: SpanStatus,
    pub attributes: BoundedMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
    Timeout,
}

// ── Active Trace Builder ─────────────────────────────────────────

pub struct TraceBuilder {
    trace_id: String,
    request_id: String,
    method: String,
    path: String,
    spans: Vec<Span>,
    span_stack: Vec<ActiveSpan>,
    started_at: Instant,
    metadata: BoundedMeta,
}

struct ActiveSpan {
    span_id: String,
    parent_id: Option<String>,
    name: String,
    started_at: Instant,
    attributes: BoundedMeta,
}

impl TraceBuilder {
    pub fn new(request_id: String, method: String, path: String) -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            request_id,
            method,
            path,
            spans: Vec::new(),
            span_stack: Vec::new(),
            started_at: Instant::now(),
            metadata: BoundedMeta::default(),
        }
    }

    pub fn start_span(&mut self, name: &str) -> String {
        let span_id = uuid::Uuid::new_v4().to_string();
        let parent_id = self.span_stack.last().map(|s| s.span_id.clone());

        self.span_stack.push(ActiveSpan {
            span_id: span_id.clone(),
            parent_id,
            name: name.to_string(),
            started_at: Instant::now(),
            attributes: BoundedMeta::default(),
        });

        span_id
    }

    pub fn end_span(&mut self, status: SpanStatus) {
        if let Some(active) = self.span_stack.pop() {
            let duration = active.started_at.elapsed();
            self.spans.push(Span {
                span_id: active.span_id,
                parent_id: active.parent_id,
                name: active.name,
                duration_us: duration.as_micros() as u64,
                status,
                attributes: active.attributes,
            });
        }
    }

    pub fn add_span_attribute(&mut self, key: &str, value: &str) {
        if let Some(ref mut span) = self.span_stack.last_mut() {
            let _ = span.attributes.insert(key.to_string(), value.to_string());
        }
    }

    pub fn add_metadata(&mut self, key: &str, value: &str) {
        let _ = self.metadata.insert(key.to_string(), value.to_string());
    }

    pub fn finish(mut self, status: u16) -> Trace {
        // Close any open spans
        while !self.span_stack.is_empty() {
            self.end_span(SpanStatus::Ok);
        }

        Trace {
            trace_id: self.trace_id,
            request_id: self.request_id,
            method: self.method,
            path: self.path,
            status,
            total_duration_us: self.started_at.elapsed().as_micros() as u64,
            spans: self.spans,
            metadata: self.metadata,
            started_at: now_millis(),
        }
    }
}

// ── Trace Store ──────────────────────────────────────────────────

pub struct TraceStore {
    traces: std::sync::RwLock<Vec<Trace>>,
    max_traces: usize,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self::new(10000)
    }
}

impl TraceStore {
    pub fn new(max_traces: usize) -> Self {
        Self {
            traces: std::sync::RwLock::new(Vec::with_capacity(max_traces)),
            max_traces,
        }
    }

    pub fn record(&self, trace: Trace) {
        let mut traces = self.traces.write().unwrap();
        if traces.len() >= self.max_traces {
            traces.remove(0);
        }
        traces.push(trace);
    }

    pub fn get(&self, trace_id: &str) -> Option<Trace> {
        let traces = self.traces.read().unwrap();
        traces.iter().find(|t| t.trace_id == trace_id).cloned()
    }

    pub fn recent(&self, n: usize) -> Vec<Trace> {
        let traces = self.traces.read().unwrap();
        traces.iter().rev().take(n).cloned().collect()
    }

    pub fn by_request(&self, request_id: &str) -> Option<Trace> {
        let traces = self.traces.read().unwrap();
        traces.iter().find(|t| t.request_id == request_id).cloned()
    }

    pub fn stats(&self) -> TraceStats {
        let traces = self.traces.read().unwrap();
        let total = traces.len();
        let errors = traces.iter().filter(|t| t.status >= 400).count();
        let avg_duration = if total > 0 {
            traces.iter().map(|t| t.total_duration_us).sum::<u64>() / total as u64
        } else {
            0
        };

        TraceStats {
            total_traces: total,
            error_traces: errors,
            avg_duration_us: avg_duration,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStats {
    pub total_traces: usize,
    pub error_traces: usize,
    pub avg_duration_us: u64,
}

// ── ASCII Visualization ──────────────────────────────────────────

pub fn render_trace_ascii(trace: &Trace) -> String {
    let mut output = String::new();

    output.push_str(&format!("Trace: {}\n", trace.trace_id));
    output.push_str(&format!("Request: {} {}\n", trace.method, trace.path));
    output.push_str(&format!(
        "Status: {} | Duration: {}ms\n",
        trace.status,
        trace.total_duration_us / 1000
    ));
    output.push_str("─".repeat(60).as_str());
    output.push('\n');

    render_spans(&mut output, &trace.spans, None, 0);

    output
}

fn render_spans(output: &mut String, spans: &[Span], parent_id: Option<&str>, depth: usize) {
    let children: Vec<&Span> = spans
        .iter()
        .filter(|s| s.parent_id.as_deref() == parent_id)
        .collect();

    for (i, span) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        let prefix = if depth == 0 {
            String::new()
        } else {
            let mut p = String::new();
            for _ in 0..depth {
                p.push_str("│   ");
            }
            if is_last {
                p.push_str("└── ");
            } else {
                p.push_str("├── ");
            }
            p
        };

        let status_icon = match span.status {
            SpanStatus::Ok => "✓",
            SpanStatus::Error => "✗",
            SpanStatus::Timeout => "⏱",
        };

        let duration_ms = span.duration_us / 1000;
        let bar_len = (duration_ms as f64).log2().max(1.0) as usize;
        let bar = "█".repeat(bar_len.min(20));

        output.push_str(&format!(
            "{}{} {} {:>6}ms {}\n",
            prefix, status_icon, span.name, duration_ms, bar
        ));

        render_spans(output, spans, Some(&span.span_id), depth + 1);
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_traces(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Query(params): axum::extract::Query<rustc_hash::FxHashMap<String, String>>,
) -> axum::Json<Vec<Trace>> {
    let n = params
        .get("n")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20)
        .min(500);
    axum::Json(state.trace_store.recent(n))
}

pub async fn handle_trace_get(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.trace_store.get(&trace_id) {
        Some(trace) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::to_value(trace).unwrap()),
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": "not found"})),
        ),
    }
}

pub async fn handle_trace_ascii(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.trace_store.get(&trace_id) {
        Some(trace) => {
            let ascii = render_trace_ascii(&trace);
            (
                axum::http::StatusCode::OK,
                [("content-type", "text/plain")],
                ascii,
            )
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            [("content-type", "text/plain")],
            "Trace not found".to_string(),
        ),
    }
}

pub async fn handle_trace_stats(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<TraceStats> {
    axum::Json(state.trace_store.stats())
}

// ── Module Router ────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/traces", axum::routing::get(handle_traces))
        .route("/traces/stats", axum::routing::get(handle_trace_stats))
        .route("/traces/{id}", axum::routing::get(handle_trace_get))
        .route("/traces/{id}/ascii", axum::routing::get(handle_trace_ascii))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_builder_lifecycle() {
        let mut builder = TraceBuilder::new(
            "req-123".into(),
            "GET".into(),
            "/v1/chat/completions".into(),
        );

        builder.start_span("hook_inject");
        builder.end_span(SpanStatus::Ok);

        builder.start_span("gateway_forward");
        builder.start_span("dns_resolve");
        builder.end_span(SpanStatus::Ok);
        builder.end_span(SpanStatus::Ok);

        let trace = builder.finish(200);

        assert_eq!(trace.request_id, "req-123");
        assert_eq!(trace.method, "GET");
        assert_eq!(trace.status, 200);
        assert_eq!(trace.spans.len(), 3);
    }

    #[test]
    fn trace_store_record_get() {
        let store = TraceStore::new(100);

        let mut builder = TraceBuilder::new("req-1".into(), "POST".into(), "/api/test".into());
        builder.start_span("test_span");
        builder.end_span(SpanStatus::Ok);

        let trace = builder.finish(200);
        let trace_id = trace.trace_id.clone();

        store.record(trace);

        let retrieved = store.get(&trace_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().request_id, "req-1");
    }

    #[test]
    fn trace_store_recent() {
        let store = TraceStore::new(100);

        for i in 0..5 {
            let builder = TraceBuilder::new(format!("req-{}", i), "GET".into(), "/test".into());
            store.record(builder.finish(200));
        }

        let recent = store.recent(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn ascii_rendering() {
        let mut builder = TraceBuilder::new("req-123".into(), "GET".into(), "/v1/chat".into());

        builder.start_span("hooks");
        builder.end_span(SpanStatus::Ok);

        builder.start_span("gateway");
        builder.start_span("dns");
        builder.end_span(SpanStatus::Ok);
        builder.end_span(SpanStatus::Ok);

        let trace = builder.finish(200);
        let ascii = render_trace_ascii(&trace);

        assert!(ascii.contains("GET /v1/chat"));
        assert!(ascii.contains("hooks"));
        assert!(ascii.contains("gateway"));
        assert!(ascii.contains("dns"));
    }

    #[test]
    fn trace_stats() {
        let store = TraceStore::new(100);

        let builder = TraceBuilder::new("req-1".into(), "GET".into(), "/test".into());
        store.record(builder.finish(200));

        let builder = TraceBuilder::new("req-2".into(), "GET".into(), "/test".into());
        store.record(builder.finish(500));

        let stats = store.stats();
        assert_eq!(stats.total_traces, 2);
        assert_eq!(stats.error_traces, 1);
    }
}
