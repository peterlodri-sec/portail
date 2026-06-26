//! Session analytics — per-session trace, token, latency, and hook tracking.
//!
//! Sessions are identified by the `x-session-id` request header.
//! Each session accumulates: request count, total tokens, cache-hit tokens,
//! response times, portail overhead, and hook injection counts.
//!
//! # v2.0 — TTL Eviction
//!
//! Sessions inactive for longer than `ttl_secs` (default 3600 = 1h) are
//! periodically purged to prevent unbounded memory growth.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use rustc_hash::FxHashMap;

// ─── data model ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub session_id: String,
    pub started_at: String,
    pub last_request_at: String,
    pub request_count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_hit_tokens: u64,
    pub total_latency_ms: u64,
    pub portail_overhead_ms: u64,
    pub hooks_injected: u64,
    pub avg_response_ms: f64,
    pub cache_hit_rate: f64,
    pub recent_requests: Vec<RequestTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTrace {
    pub timestamp: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub latency_ms: u64,
    pub portail_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_hit: bool,
    pub hooks_applied: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub request_count: u64,
    pub avg_response_ms: f64,
    pub total_tokens: u64,
    pub last_seen: String,
}

// ─── store ────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<SessionStoreInner>,
}

struct SessionStoreInner {
    sessions: std::sync::RwLock<FxHashMap<String, SessionStats>>,
    recent_traces: std::sync::RwLock<Vec<RequestTrace>>,
    max_traces_per_session: usize,
}

impl SessionStore {
    pub fn new(max_traces: usize) -> Self {
        Self {
            inner: Arc::new(SessionStoreInner {
                sessions: std::sync::RwLock::new(FxHashMap::default()),
                recent_traces: std::sync::RwLock::new(Vec::new()),
                max_traces_per_session: max_traces,
            }),
        }
    }

    /// v2.0: Spawn a TTL eviction background task. Call once after creation
    /// in a tokio runtime context. Not called from tests.
    pub fn spawn_eviction(self, ttl_secs: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600));
            loop {
                interval.tick().await;
                self.evict_expired(ttl_secs);
            }
        });
    }

    fn evict_expired(&self, ttl_secs: u64) {
        let now = chrono::Utc::now().timestamp();
        let mut sessions = self
            .inner
            .sessions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let before = sessions.len();
        sessions.retain(|_, s| {
            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&s.last_request_at) {
                (now - ts.timestamp()) < ttl_secs as i64
            } else {
                true // keep if timestamp unreadable
            }
        });
        let after = sessions.len();
        if before > after {
            tracing::info!(
                evicted = before - after,
                remaining = after,
                "session TTL eviction"
            );
        }
    }

    pub fn record_request(
        &self,
        session_id: &str,
        method: &str,
        path: &str,
        status: u16,
        total_latency: Duration,
        portail_overhead: Duration,
        input_tokens: u64,
        output_tokens: u64,
        cache_hit: bool,
        hooks_applied: u64,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let latency_ms = total_latency.as_millis() as u64;
        let portail_ms = portail_overhead.as_millis() as u64;

        let trace = RequestTrace {
            timestamp: now.clone(),
            method: method.to_string(),
            path: path.to_string(),
            status,
            latency_ms,
            portail_ms,
            input_tokens,
            output_tokens,
            cache_hit,
            hooks_applied,
        };

        // Store trace
        {
            let mut traces = self.inner.recent_traces.write().unwrap();
            traces.push(trace.clone());
            if traces.len() > self.inner.max_traces_per_session * 10 {
                traces.remove(0);
            }
        }

        // Update session
        let mut sessions = self.inner.sessions.write().unwrap();
        let session = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionStats {
                session_id: session_id.to_string(),
                started_at: now.clone(),
                last_request_at: now.clone(),
                request_count: 0,
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_hit_tokens: 0,
                total_latency_ms: 0,
                portail_overhead_ms: 0,
                hooks_injected: 0,
                avg_response_ms: 0.0,
                cache_hit_rate: 0.0,
                recent_requests: Vec::new(),
            });

        session.request_count += 1;
        session.last_request_at = now;
        session.total_input_tokens += input_tokens;
        session.total_output_tokens += output_tokens;
        if cache_hit {
            session.total_cache_hit_tokens += input_tokens;
        }
        session.total_latency_ms += latency_ms;
        session.portail_overhead_ms += portail_ms;
        session.hooks_injected += hooks_applied;
        session.avg_response_ms = session.total_latency_ms as f64 / session.request_count as f64;
        session.cache_hit_rate = if session.total_input_tokens > 0 {
            session.total_cache_hit_tokens as f64 / session.total_input_tokens as f64
        } else {
            0.0
        };

        session.recent_requests.push(trace);
        if session.recent_requests.len() > self.inner.max_traces_per_session {
            session.recent_requests.remove(0);
        }
    }

    pub fn get_session(&self, session_id: &str) -> Option<SessionStats> {
        self.inner.sessions.read().unwrap().get(session_id).cloned()
    }

    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        self.inner
            .sessions
            .read()
            .unwrap()
            .values()
            .map(|s| SessionSummary {
                session_id: s.session_id.clone(),
                request_count: s.request_count,
                avg_response_ms: s.avg_response_ms,
                total_tokens: s.total_input_tokens + s.total_output_tokens,
                last_seen: s.last_request_at.clone(),
            })
            .collect()
    }
}

// ─── axum handlers ────────────────────────────────────────────────

pub async fn handle_list_sessions(
    axum::extract::State(store): axum::extract::State<SessionStore>,
) -> axum::Json<Vec<SessionSummary>> {
    axum::Json(store.list_sessions())
}

pub async fn handle_get_session(
    axum::extract::State(store): axum::extract::State<SessionStore>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<SessionStats>, axum::http::StatusCode> {
    store
        .get_session(&id)
        .map(axum::Json)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}

pub fn router() -> axum::Router<SessionStore> {
    axum::Router::new()
        .route("/sessions", axum::routing::get(handle_list_sessions))
        .route("/sessions/{id}", axum::routing::get(handle_get_session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_is_empty() {
        let store = SessionStore::new(10);
        assert!(store.list_sessions().is_empty());
    }

    #[test]
    fn record_and_get_session() {
        let store = SessionStore::new(10);
        store.record_request(
            "sess-1",
            "GET",
            "/test",
            200,
            std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(5),
            100,
            200,
            false,
            0,
        );
        let stats = store.get_session("sess-1");
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().request_count, 1);
    }

    #[test]
    fn list_sessions_returns_summaries() {
        let store = SessionStore::new(10);
        store.record_request(
            "s1",
            "GET",
            "/a",
            200,
            std::time::Duration::from_millis(10),
            std::time::Duration::from_millis(1),
            0,
            0,
            false,
            0,
        );
        store.record_request(
            "s2",
            "POST",
            "/b",
            201,
            std::time::Duration::from_millis(20),
            std::time::Duration::from_millis(2),
            10,
            20,
            true,
            1,
        );
        let list = store.list_sessions();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn missing_session_returns_none() {
        let store = SessionStore::new(10);
        assert!(store.get_session("nonexistent").is_none());
    }
}
