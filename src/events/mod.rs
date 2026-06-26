use crate::hooks::Hook;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::SystemTime;
use tokio::sync::broadcast;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentEvent {
    pub agent_id: String,
    pub event_type: String,
    pub severity: String,
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub metadata: FxHashMap<String, String>,
}

pub struct EventLog {
    ring: Mutex<VecDeque<AgentEvent>>,
    tx: broadcast::Sender<AgentEvent>,
    max_events: usize,
}

impl EventLog {
    pub fn new(max_events: usize) -> Self {
        let (tx, _) = broadcast::channel(2048);
        Self { ring: Mutex::new(VecDeque::new()), tx, max_events }
    }

    pub fn publish(&self, mut event: AgentEvent) {
        if event.timestamp == 0 {
            event.timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
        }
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        if ring.len() >= self.max_events {
            ring.pop_front();
        }
        ring.push_back(event.clone());
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }

    pub fn recent(&self, n: usize) -> Vec<AgentEvent> {
        let ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        ring.iter().rev().take(n).cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.ring.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// Inject hook content into an event's metadata under `_hook` key.
fn apply_event_hooks(event: &mut AgentEvent, hooks: &[Hook]) {
    for hook in hooks {
        event
            .metadata
            .entry("_hook".into())
            .or_default()
            .push_str(&hook.content);
    }
}

pub async fn handle_publish(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::Json(mut event): axum::Json<AgentEvent>,
) -> impl axum::response::IntoResponse {
    let hooks = state.hooks.match_event(&event.agent_id, &event.event_type);
    if !hooks.is_empty() {
        apply_event_hooks(&mut event, &hooks);
    }
    state.event_log.publish(event);
    (axum::http::StatusCode::ACCEPTED, "accepted")
}

pub async fn handle_recent(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::extract::Query(params): axum::extract::Query<FxHashMap<String, String>>,
) -> axum::Json<Vec<AgentEvent>> {
    let n = params.get("n").and_then(|v| v.parse().ok()).unwrap_or(50).min(500);
    axum::Json(state.event_log.recent(n))
}

pub async fn handle_stream(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::{Event, KeepAlive};
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt;

    let rx = state.event_log.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|r| match r {
        Ok(event) => {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(Event::default().data(data)))
        }
        Err(_) => None,
    });

    axum::response::Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(i: u64) -> AgentEvent {
        AgentEvent {
            agent_id: "test".into(),
            event_type: "ping".into(),
            severity: "info".into(),
            timestamp: i,
            metadata: FxHashMap::default(),
        }
    }

    #[test]
    fn event_log_ring() {
        let log = EventLog::new(3);
        for i in 0..5 {
            log.publish(make_event(i));
        }
        let recent = log.recent(10);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].timestamp, 4);
        assert_eq!(recent[2].timestamp, 2);
    }

    #[test]
    fn event_log_empty() {
        let log = EventLog::new(100);
        assert!(log.recent(10).is_empty());
    }

    #[test]
    fn event_log_timestamp_auto() {
        let log = EventLog::new(10);
        log.publish(AgentEvent {
            agent_id: "a".into(),
            event_type: "start".into(),
            severity: "info".into(),
            timestamp: 0,
            metadata: FxHashMap::default(),
        });
        let recent = log.recent(1);
        assert!(recent[0].timestamp > 0);
    }

    #[tokio::test]
    async fn event_log_broadcast() {
        let log = std::sync::Arc::new(EventLog::new(100));
        let mut rx = log.subscribe();
        log.publish(AgentEvent {
            agent_id: "a".into(),
            event_type: "test".into(),
            severity: "info".into(),
            timestamp: 1,
            metadata: FxHashMap::default(),
        });
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.event_type, "test");
    }
}
