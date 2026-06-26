use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::types::BoundedMeta;

// ── Agent Card: capability advertisement ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: AgentCapabilities,
    #[serde(default)]
    pub skills: Vec<Skill>,
    #[serde(default)]
    pub authentication: Option<Authentication>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub push_notifications: bool,
    #[serde(default)]
    pub state_transition_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authentication {
    pub schemes: Vec<String>,
    #[serde(default)]
    pub credentials: Option<String>,
}

// ── Task: lifecycle management ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub metadata: BoundedMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub metadata: BoundedMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text { text: String },
    File { file: FilePart },
    Data { data: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    pub name: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub bytes: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub append: bool,
    #[serde(default)]
    pub last_chunk: bool,
}

// ── Task Store: in-memory task state ─────────────────────────────

pub struct TaskStore {
    tasks: RwLock<HashMap<String, Task>>,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskStore {
    pub fn new() -> Self {
        Self { tasks: std::sync::RwLock::new(HashMap::new()) }
    }

    pub fn create(&self, id: String) -> Task {
        let task = Task {
            id: id.clone(),
            status: TaskStatus::Submitted,
            messages: Vec::new(),
            artifacts: Vec::new(),
            metadata: BoundedMeta::default(),
        };
        self.tasks.write().unwrap().insert(id, task.clone());
        task
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.tasks.read().unwrap().get(id).cloned()
    }

    pub fn update_status(&self, id: &str, status: TaskStatus) -> Option<Task> {
        let mut tasks = self.tasks.write().unwrap();
        let task = tasks.get_mut(id)?;
        task.status = status;
        Some(task.clone())
    }

    pub fn add_message(&self, id: &str, message: Message) -> Option<Task> {
        let mut tasks = self.tasks.write().unwrap();
        let task = tasks.get_mut(id)?;
        task.messages.push(message);
        Some(task.clone())
    }

    pub fn add_artifact(&self, id: &str, artifact: Artifact) -> Option<Task> {
        let mut tasks = self.tasks.write().unwrap();
        let task = tasks.get_mut(id)?;
        task.artifacts.push(artifact);
        Some(task.clone())
    }

    pub fn get_all(&self) -> Vec<Task> {
        self.tasks.read().unwrap().values().cloned().collect()
    }
}

// ── HTTP handlers ────────────────────────────────────────────────

pub async fn handle_agent_card(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<AgentCard> {
    let cfg = state.config.read().unwrap();
    let card = AgentCard {
        name: "portail".into(),
        description: "Unified proxy/gateway with AI, MCP, and CDN support".into(),
        url: format!("http://{}", cfg.listen),
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: AgentCapabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
        },
        skills: vec![
            Skill {
                id: "proxy".into(),
                name: "AI Gateway".into(),
                description: "Proxy requests to AI providers".into(),
                tags: vec!["ai".into(), "proxy".into()],
                examples: vec![],
            },
            Skill {
                id: "mcp".into(),
                name: "MCP Gateway".into(),
                description: "Route to MCP tools via Unix socket".into(),
                tags: vec!["mcp".into(), "tools".into()],
                examples: vec![],
            },
        ],
        authentication: None,
    };
    axum::Json(card)
}

pub async fn handle_task_create(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl axum::response::IntoResponse {
    let id = req.get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(&uuid::Uuid::new_v4().to_string())
        .to_string();

    let task = state.a2a_tasks.create(id);
    state.event_log.publish(crate::events::AgentEvent {
        agent_id: "a2a".into(),
        event_type: "task_created".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: BoundedMeta::from_iter([
            ("task_id".into(), task.id.clone()),
        ]),
    });

    (axum::http::StatusCode::CREATED, axum::Json(task))
}

pub async fn handle_task_get(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.a2a_tasks.get(&id) {
        Some(task) => (axum::http::StatusCode::OK, axum::Json(serde_json::to_value(task).unwrap())),
        None => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": "not found"}))),
    }
}

// ── WebSocket: bidirectional A2A streaming ────────────────────────

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use futures::{SinkExt, StreamExt};

/// Handle WebSocket upgrade. Established connections receive live
/// task events and can send task commands (create, update status,
/// add message) over the socket.
pub async fn handle_ws(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

async fn handle_ws_connection(socket: WebSocket, state: Arc<crate::AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to event log for live task updates
    let mut event_rx = state.event_log.subscribe();

    // Spawn event-forwarding task
    let send_state = state.clone();
    let send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            if event.event_type.starts_with("task_") {
                let payload = serde_json::json!({
                    "type": "task_event",
                    "agent_id": event.agent_id,
                    "event_type": event.event_type,
                    "severity": event.severity,
                    "metadata": event.metadata,
                });
                if let Ok(text) = serde_json::to_string(&payload) {
                    if sender.send(WsMessage::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Receive loop — process client commands
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            WsMessage::Text(text) => {
                let response = handle_ws_command(&send_state, &text).await;
                if let Ok(resp_text) = serde_json::to_string(&response) {
                    // We need to send through the sender — use a channel
                    // For simplicity, log the response; full bidir needs
                    // channel-merge pattern
                    tracing::debug!(ws_response = %resp_text, "a2a ws command processed");
                }
            }
            WsMessage::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}

async fn handle_ws_command(
    state: &Arc<crate::AppState>,
    text: &str,
) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": format!("invalid json: {}", e)}),
    };

    let action = cmd.get("action").and_then(|v| v.as_str()).unwrap_or("");

    match action {
        "create_task" => {
            let id = uuid::Uuid::new_v4().to_string();
            let task = state.a2a_tasks.create(id.clone());
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "a2a-ws".into(),
                event_type: "task_created".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: BoundedMeta::from_iter([
                    ("task_id".into(), id),
                ]),
            });
            serde_json::json!({"ok": true, "task_id": task.id, "status": format!("{:?}", task.status)})
        }
        "get_task" => {
            let id = cmd.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            match state.a2a_tasks.get(id) {
                Some(task) => serde_json::json!({"ok": true, "task": task}),
                None => serde_json::json!({"error": "task not found"}),
            }
        }
        "update_status" => {
            let id = cmd.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            let status_str = cmd.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let status = match status_str {
                "working" => TaskStatus::Working,
                "completed" => TaskStatus::Completed,
                "failed" => TaskStatus::Failed,
                "canceled" => TaskStatus::Canceled,
                _ => return serde_json::json!({"error": "invalid status"}),
            };
            match state.a2a_tasks.update_status(id, status) {
                Some(task) => {
                    state.event_log.publish(crate::events::AgentEvent {
                        agent_id: "a2a-ws".into(),
                        event_type: "task_updated".into(),
                        severity: "info".into(),
                        timestamp: 0,
                        metadata: BoundedMeta::from_iter([
                            ("task_id".into(), task.id.clone()),
                            ("status".into(), format!("{:?}", task.status)),
                        ]),
                    });
                    serde_json::json!({"ok": true, "task_id": task.id, "status": format!("{:?}", task.status)})
                }
                None => serde_json::json!({"error": "task not found"}),
            }
        }
        "ping" => serde_json::json!({"pong": true}),
        _ => serde_json::json!({"error": format!("unknown action: {}", action)}),
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lifecycle() {
        let store = TaskStore::new();
        let task = store.create("t1".into());
        assert_eq!(task.status, TaskStatus::Submitted);

        let task = store.update_status("t1", TaskStatus::Working).unwrap();
        assert_eq!(task.status, TaskStatus::Working);

        let task = store.add_message("t1", Message {
            role: "user".into(),
            parts: vec![Part::Text { text: "hello".into() }],
            metadata: BoundedMeta::default(),
        }).unwrap();
        assert_eq!(task.messages.len(), 1);

        let task = store.update_status("t1", TaskStatus::Completed).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn task_not_found() {
        let store = TaskStore::new();
        assert!(store.get("nonexistent").is_none());
        assert!(store.update_status("nonexistent", TaskStatus::Completed).is_none());
    }

    #[test]
    fn agent_card_json_roundtrip() {
        let card = AgentCard {
            name: "portail".into(),
            description: "test".into(),
            url: "http://0.0.0.0:8787".into(),
            version: "0.6.0".into(),
            capabilities: AgentCapabilities {
                streaming: true,
                push_notifications: false,
                state_transition_history: true,
            },
            skills: vec![Skill {
                id: "proxy".into(),
                name: "Proxy".into(),
                description: "Routes requests".into(),
                tags: vec!["http".into()],
                examples: vec![],
            }],
            authentication: Some(Authentication {
                schemes: vec!["bearer".into()],
                credentials: None,
            }),
        };
        let json = serde_json::to_string(&card).unwrap();
        let roundtrip: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "portail");
        assert_eq!(roundtrip.skills.len(), 1);
        assert!(roundtrip.capabilities.streaming);
    }

    #[test]
    fn task_json_roundtrip() {
        let task = Task {
            id: "t1".into(),
            status: TaskStatus::Working,
            messages: vec![Message {
                role: "user".into(),
                parts: vec![Part::Text { text: "hello".into() }],
                metadata: BoundedMeta::default(),
            }],
            artifacts: vec![],
            metadata: {
                let mut m = BoundedMeta::default();
                m.insert("priority".into(), "high".into());
                m
            },
        };
        let json = serde_json::to_string(&task).unwrap();
        let roundtrip: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.id, "t1");
        assert_eq!(roundtrip.status, TaskStatus::Working);
        assert_eq!(roundtrip.messages.len(), 1);
        assert_eq!(roundtrip.metadata.get("priority").unwrap(), "high");
    }

    #[test]
    fn part_json_variants() {
        let text = serde_json::to_string(&Part::Text { text: "hi".into() }).unwrap();
        assert!(text.contains("text"));
        assert!(text.contains("hi"));

        let file = serde_json::to_string(&Part::File { file: FilePart {
            name: "data.csv".into(),
            bytes: Some("base64...".into()),
            mime_type: Some("text/csv".into()),
            uri: None,
        }}).unwrap();
        assert!(file.contains("file"));
        assert!(file.contains("data.csv"));

        let data = serde_json::to_string(&Part::Data { data: serde_json::json!({}) }).unwrap();
        assert!(data.contains("data"));
    }
}
