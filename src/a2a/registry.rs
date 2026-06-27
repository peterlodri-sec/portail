//! A2A Agent Registry — skill-based discovery and routing.
//!
//! External agents register their `AgentCard` plus a callback URL. The gateway
//! stores them in-memory (with optional persistence via `event_store`) and can
//! route A2A tasks to the agent whose skills best match the request.

use super::AgentCard;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Registered agent entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredAgent {
    pub id: String,
    pub card: AgentCard,
    pub url: String,
    #[serde(default)]
    pub last_seen_at: i64,
}

/// Request to register an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub id: String,
    pub card: AgentCard,
    pub url: String,
}

/// In-memory agent registry with skill indexing.
#[derive(Clone)]
pub struct AgentRegistry {
    inner: Arc<RwLock<AgentRegistryInner>>,
}

struct AgentRegistryInner {
    agents: HashMap<String, RegisteredAgent>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(AgentRegistryInner {
                agents: HashMap::new(),
            })),
        }
    }

    pub fn register(&self, req: RegisterRequest) -> RegisteredAgent {
        let agent = RegisteredAgent {
            id: req.id.clone(),
            card: req.card,
            url: req.url,
            last_seen_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        };
        self.inner
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .agents
            .insert(req.id, agent.clone());
        agent
    }

    pub fn deregister(&self, id: &str) -> bool {
        self.inner
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .agents
            .remove(id)
            .is_some()
    }

    pub fn get(&self, id: &str) -> Option<RegisteredAgent> {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).agents.get(id).cloned()
    }

    pub fn list(&self) -> Vec<RegisteredAgent> {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .agents
            .values()
            .cloned()
            .collect()
    }

    /// Find the best matching agent for a requested skill tag.
    /// Returns the agent whose skills contain the tag, preferring exact
    /// skill-id matches then tag substring matches.
    pub fn find_by_skill(&self, tag: &str) -> Option<RegisteredAgent> {
        let agents = self.list();
        agents
            .into_iter()
            .filter(|a| {
                a.card.skills.iter().any(|s| {
                    s.id.eq_ignore_ascii_case(tag)
                        || s.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
                        || s.name.eq_ignore_ascii_case(tag)
                })
            })
            .max_by_key(|a| {
                // Prefer exact skill-id matches.
                a.card
                    .skills
                    .iter()
                    .any(|s| s.id.eq_ignore_ascii_case(tag)) as u8
            })
    }
}

// ─── HTTP handlers ────────────────────────────────────────────────

pub async fn handle_register(
    State(state): State<Arc<crate::AppState>>,
    Json(req): Json<RegisterRequest>,
) -> impl axum::response::IntoResponse {
    let agent = state.a2a_registry.register(req);
    (StatusCode::CREATED, Json(serde_json::json!({ "agent": agent })))
}

pub async fn handle_deregister(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    if state.a2a_registry.deregister(&id) {
        (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "agent not found" })),
        )
    }
}

pub async fn handle_list(
    State(state): State<Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    Json(serde_json::json!({ "agents": state.a2a_registry.list() }))
}

pub async fn handle_get(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    match state.a2a_registry.get(&id) {
        Some(agent) => (StatusCode::OK, Json(serde_json::json!({ "agent": agent }))),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "agent not found" })),
        ),
    }
}
