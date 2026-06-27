use crate::types::BoundedMeta;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use tokio_stream::wrappers::ReceiverStream;

// ── JSON-RPC 2.0 types ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }
    fn invalid_params(msg: &str) -> Self {
        Self {
            code: -32602,
            message: msg.to_string(),
            data: None,
        }
    }
    fn task_not_found(id: &str) -> Self {
        Self {
            code: -32001,
            message: format!("Task not found: {id}"),
            data: None,
        }
    }
    #[allow(dead_code)]
    fn internal(msg: &str) -> Self {
        Self {
            code: -32603,
            message: msg.to_string(),
            data: None,
        }
    }
}

fn rpc_result(id: Option<serde_json::Value>, result: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    }
}

fn rpc_error(id: Option<serde_json::Value>, error: JsonRpcError) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(error),
    }
}

// ── A2A Protocol types ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Rejected,
    AuthRequired,
    Unknown,
}

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
    #[serde(default)]
    pub default_input_modes: Vec<String>,
    #[serde(default)]
    pub default_output_modes: Vec<String>,
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

// ── Task lifecycle ────────────────────────────────────────────────

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
    #[serde(default)]
    pub push_notification_config: Option<PushNotificationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub timestamp: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    pub url: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub authentication: Option<Authentication>,
}

// ── Task Store ────────────────────────────────────────────────────

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
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    pub fn create(&self, id: String) -> Task {
        let task = Task {
            id: id.clone(),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
            },
            messages: Vec::new(),
            artifacts: Vec::new(),
            metadata: BoundedMeta::default(),
            push_notification_config: None,
        };
        self.tasks.write().unwrap().insert(id, task.clone());
        task
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.tasks.read().unwrap().get(id).cloned()
    }

    pub fn update_state(&self, id: &str, state: TaskState) -> Option<Task> {
        let mut tasks = self.tasks.write().unwrap();
        let task = tasks.get_mut(id)?;
        task.status.state = state;
        task.status.timestamp = Some(chrono::Utc::now().to_rfc3339());
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

    pub fn set_push_config(&self, id: &str, config: PushNotificationConfig) -> Option<Task> {
        let mut tasks = self.tasks.write().unwrap();
        let task = tasks.get_mut(id)?;
        task.push_notification_config = Some(config);
        Some(task.clone())
    }

    pub fn get_push_config(&self, id: &str) -> Option<PushNotificationConfig> {
        let tasks = self.tasks.read().unwrap();
        tasks.get(id)?.push_notification_config.clone()
    }

    pub fn get_all(&self) -> Vec<Task> {
        self.tasks.read().unwrap().values().cloned().collect()
    }
}

// ── HTTP handlers ────────────────────────────────────────────────

/// GET /.well-known/agent.json — Agent Card discovery
pub async fn handle_agent_card(State(state): State<Arc<crate::AppState>>) -> axum::Json<AgentCard> {
    let cfg = state.config.read().unwrap();
    let card = AgentCard {
        name: "portail".into(),
        description: "Unified proxy/gateway with AI, MCP, A2A, and CDN support".into(),
        url: format!("http://{}", cfg.listen),
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: AgentCapabilities {
            streaming: true,
            push_notifications: true,
            state_transition_history: true,
        },
        skills: vec![
            Skill {
                id: "proxy".into(),
                name: "AI Gateway".into(),
                description: "Proxy requests to AI providers (OpenAI, Anthropic, Google, Ollama)"
                    .into(),
                tags: vec!["ai".into(), "proxy".into()],
                examples: vec!["Summarize this document".into(), "Generate code".into()],
            },
            Skill {
                id: "mcp".into(),
                name: "MCP Gateway".into(),
                description: "Route to MCP tools via Unix socket (Python or WASM)".into(),
                tags: vec!["mcp".into(), "tools".into()],
                examples: vec!["List available tools".into(), "Call a tool".into()],
            },
        ],
        authentication: None,
        default_input_modes: vec!["text".into(), "application/json".into()],
        default_output_modes: vec!["text".into(), "application/json".into()],
    };
    axum::Json(card)
}

/// POST /a2a — JSON-RPC 2.0 endpoint (all A2A methods)
pub async fn handle_rpc(
    State(state): State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<JsonRpcRequest>,
) -> Response {
    if req.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            axum::Json(rpc_error(
                req.id,
                JsonRpcError {
                    code: -32600,
                    message: "Invalid Request: jsonrpc must be '2.0'".into(),
                    data: None,
                },
            )),
        )
            .into_response();
    }

    let resp = match req.method.as_str() {
        "tasks/send" => handle_tasks_send(&state, &req.id, &req.params).await,
        "tasks/get" => handle_tasks_get(&state, &req.id, &req.params),
        "tasks/cancel" => handle_tasks_cancel(&state, &req.id, &req.params),
        "tasks/pushNotification/set" => handle_push_set(&state, &req.id, &req.params),
        "tasks/pushNotification/get" => handle_push_get(&state, &req.id, &req.params),
        _ => rpc_error(req.id, JsonRpcError::method_not_found(&req.method)),
    };

    (StatusCode::OK, axum::Json(resp)).into_response()
}

/// POST /a2a — SSE streaming endpoint (tasks/sendSubscribe)
pub async fn handle_rpc_stream(
    State(state): State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<JsonRpcRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel(64);

    if req.method != "tasks/sendSubscribe" {
        let err = rpc_error(req.id.clone(), JsonRpcError::method_not_found(&req.method));
        let _ = tx
            .send(Ok(
                Event::default().data(serde_json::to_string(&err).unwrap())
            ))
            .await;
        return Sse::new(ReceiverStream::new(rx));
    }

    let task_id = req
        .params
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Create or get task
    let task = state
        .a2a_tasks
        .get(&task_id)
        .unwrap_or_else(|| state.a2a_tasks.create(task_id.clone()));

    // Send initial task
    let _ = tx
        .send(Ok(
            Event::default().data(serde_json::to_string(&task).unwrap())
        ))
        .await;

    // Spawn background work — transition to working, then completed
    let store = state.a2a_tasks.clone();
    let events = state.event_log.clone();
    tokio::spawn(async move {
        // Transition: working
        if let Some(task) = store.update_state(&task_id, TaskState::Working) {
            let _ = tx
                .send(Ok(
                    Event::default().data(serde_json::to_string(&task).unwrap())
                ))
                .await;
            events.publish(crate::events::AgentEvent {
                agent_id: "a2a".into(),
                event_type: "task_working".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: BoundedMeta::from_iter([("task_id".into(), task_id.clone())]),
            });
        }

        // Simulate processing (replace with actual agent logic)
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Transition: completed
        if let Some(task) = store.update_state(&task_id, TaskState::Completed) {
            let _ = tx
                .send(Ok(
                    Event::default().data(serde_json::to_string(&task).unwrap())
                ))
                .await;
            events.publish(crate::events::AgentEvent {
                agent_id: "a2a".into(),
                event_type: "task_completed".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: BoundedMeta::from_iter([("task_id".into(), task_id)]),
            });
        }
    });

    Sse::new(ReceiverStream::new(rx))
}

// ── Method handlers ──────────────────────────────────────────────

async fn handle_tasks_send(
    state: &Arc<crate::AppState>,
    id: &Option<serde_json::Value>,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let task_id = params
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let task = state
        .a2a_tasks
        .get(&task_id)
        .unwrap_or_else(|| state.a2a_tasks.create(task_id.clone()));

    // Extract message from params
    if let Some(msg) = params.get("message") {
        let message = Message {
            role: msg
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("user")
                .into(),
            parts: serde_json::from_value(msg.get("parts").cloned().unwrap_or_default())
                .unwrap_or_default(),
            metadata: BoundedMeta::default(),
        };
        state.a2a_tasks.add_message(&task.id, message);
    }

    // Transition to working
    let task = state
        .a2a_tasks
        .update_state(&task.id, TaskState::Working)
        .unwrap_or(task);

    state.event_log.publish(crate::events::AgentEvent {
        agent_id: "a2a".into(),
        event_type: "task_received".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: BoundedMeta::from_iter([("task_id".into(), task.id.clone())]),
    });

    rpc_result(id.clone(), serde_json::to_value(task).unwrap())
}

fn handle_tasks_get(
    state: &Arc<crate::AppState>,
    id: &Option<serde_json::Value>,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let task_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(tid) => tid,
        None => return rpc_error(id.clone(), JsonRpcError::invalid_params("missing 'id'")),
    };

    match state.a2a_tasks.get(task_id) {
        Some(task) => rpc_result(id.clone(), serde_json::to_value(task).unwrap()),
        None => rpc_error(id.clone(), JsonRpcError::task_not_found(task_id)),
    }
}

fn handle_tasks_cancel(
    state: &Arc<crate::AppState>,
    id: &Option<serde_json::Value>,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let task_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(tid) => tid,
        None => return rpc_error(id.clone(), JsonRpcError::invalid_params("missing 'id'")),
    };

    match state.a2a_tasks.update_state(task_id, TaskState::Canceled) {
        Some(task) => {
            state.event_log.publish(crate::events::AgentEvent {
                agent_id: "a2a".into(),
                event_type: "task_canceled".into(),
                severity: "info".into(),
                timestamp: 0,
                metadata: BoundedMeta::from_iter([("task_id".into(), task_id.to_string())]),
            });
            rpc_result(id.clone(), serde_json::to_value(task).unwrap())
        }
        None => rpc_error(id.clone(), JsonRpcError::task_not_found(task_id)),
    }
}

fn handle_push_set(
    state: &Arc<crate::AppState>,
    id: &Option<serde_json::Value>,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let task_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(tid) => tid,
        None => return rpc_error(id.clone(), JsonRpcError::invalid_params("missing 'id'")),
    };

    let config: PushNotificationConfig = match serde_json::from_value(
        params
            .get("pushNotificationConfig")
            .cloned()
            .unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => return rpc_error(id.clone(), JsonRpcError::invalid_params(&e.to_string())),
    };

    match state.a2a_tasks.set_push_config(task_id, config) {
        Some(task) => rpc_result(id.clone(), serde_json::to_value(task).unwrap()),
        None => rpc_error(id.clone(), JsonRpcError::task_not_found(task_id)),
    }
}

fn handle_push_get(
    state: &Arc<crate::AppState>,
    id: &Option<serde_json::Value>,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let task_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(tid) => tid,
        None => return rpc_error(id.clone(), JsonRpcError::invalid_params("missing 'id'")),
    };

    match state.a2a_tasks.get_push_config(task_id) {
        Some(config) => rpc_result(id.clone(), serde_json::to_value(config).unwrap()),
        None => rpc_error(id.clone(), JsonRpcError::task_not_found(task_id)),
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
        assert_eq!(task.status.state, TaskState::Submitted);

        let task = store.update_state("t1", TaskState::Working).unwrap();
        assert_eq!(task.status.state, TaskState::Working);

        let task = store
            .add_message(
                "t1",
                Message {
                    role: "user".into(),
                    parts: vec![Part::Text {
                        text: "hello".into(),
                    }],
                    metadata: BoundedMeta::default(),
                },
            )
            .unwrap();
        assert_eq!(task.messages.len(), 1);

        let task = store.update_state("t1", TaskState::Completed).unwrap();
        assert_eq!(task.status.state, TaskState::Completed);
    }

    #[test]
    fn task_cancel() {
        let store = TaskStore::new();
        store.create("t1".into());
        let task = store.update_state("t1", TaskState::Canceled).unwrap();
        assert_eq!(task.status.state, TaskState::Canceled);
    }

    #[test]
    fn task_not_found() {
        let store = TaskStore::new();
        assert!(store.get("nonexistent").is_none());
        assert!(
            store
                .update_state("nonexistent", TaskState::Completed)
                .is_none()
        );
    }

    #[test]
    fn push_notification_roundtrip() {
        let store = TaskStore::new();
        store.create("t1".into());
        let config = PushNotificationConfig {
            url: "https://example.com/hook".into(),
            token: Some("tok_abc".into()),
            authentication: None,
        };
        store.set_push_config("t1", config.clone());
        let got = store.get_push_config("t1").unwrap();
        assert_eq!(got.url, "https://example.com/hook");
        assert_eq!(got.token, Some("tok_abc".into()));
    }

    #[test]
    fn agent_card_json_roundtrip() {
        let card = AgentCard {
            name: "portail".into(),
            description: "test".into(),
            url: "http://0.0.0.0:8787".into(),
            version: "2.1.0".into(),
            capabilities: AgentCapabilities {
                streaming: true,
                push_notifications: true,
                state_transition_history: true,
            },
            skills: vec![Skill {
                id: "proxy".into(),
                name: "Proxy".into(),
                description: "Routes requests".into(),
                tags: vec!["http".into()],
                examples: vec![],
            }],
            authentication: None,
            default_input_modes: vec!["text".into()],
            default_output_modes: vec!["text".into()],
        };
        let json = serde_json::to_string(&card).unwrap();
        let roundtrip: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "portail");
        assert_eq!(roundtrip.skills.len(), 1);
        assert!(roundtrip.capabilities.streaming);
        assert!(roundtrip.capabilities.push_notifications);
    }

    #[test]
    fn jsonrpc_request_roundtrip() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "tasks/send".into(),
            params: serde_json::json!({"id": "t1", "message": {"role": "user", "parts": []}}),
        };
        let json = serde_json::to_string(&req).unwrap();
        let roundtrip: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.method, "tasks/send");
        assert_eq!(roundtrip.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn jsonrpc_response_error() {
        let resp = rpc_error(
            Some(serde_json::json!(1)),
            JsonRpcError::method_not_found("foo"),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32601"));
        assert!(json.contains("foo"));
    }
}
