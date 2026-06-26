use serde::{Deserialize, Serialize};
use std::sync::Arc;
use rustc_hash::FxHashMap;

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
    pub metadata: FxHashMap<String, String>,
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
    pub metadata: FxHashMap<String, String>,
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
    tasks: std::sync::RwLock<FxHashMap<String, Task>>,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskStore {
    pub fn new() -> Self {
        Self { tasks: std::sync::RwLock::new(FxHashMap::default()) }
    }

    pub fn create(&self, id: String) -> Task {
        let task = Task {
            id: id.clone(),
            status: TaskStatus::Submitted,
            messages: Vec::new(),
            artifacts: Vec::new(),
            metadata: FxHashMap::default(),
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
        metadata: rustc_hash::FxHashMap::from_iter([
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

// ── Module-level router ──────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/.well-known/agent.json", axum::routing::get(handle_agent_card))
        .route("/a2a/tasks", axum::routing::post(handle_task_create))
        .route("/a2a/tasks/{id}", axum::routing::get(handle_task_get))
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
            metadata: FxHashMap::default(),
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
}
