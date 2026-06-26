use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub result: serde_json::Value,
    #[serde(default)]
    pub is_error: bool,
}

pub async fn handle_chat(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<ChatRequest>,
) -> axum::response::Response {
    let upstream = {
        let c = state.config.read().unwrap();
        c.ai_gateway
            .as_ref()
            .filter(|g| g.enabled)
            .map(|g| g.upstream.clone())
    };

    let Some(upstream) = upstream else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "error": "AI gateway not configured"
            })),
        )
            .into_response();
    };

    let last_msg = req.messages.last().map(|m| m.content.as_str()).unwrap_or("");
    let is_orchestrator = last_msg.starts_with("/orchestrate")
        || last_msg.starts_with("/plan")
        || last_msg.starts_with("/delegate");

    if is_orchestrator {
        return handle_orchestrator_command(last_msg, state).await;
    }

    let body_bytes = serde_json::to_vec(&req).unwrap_or_default();
    match crate::gateway::forward_with_url(&upstream, "/v1/chat/completions", &body_bytes).await {
        Ok(resp) => {
            let status = resp.status();
            let body = axum::body::to_bytes(resp.into_body(), 10_000_000).await.unwrap_or_default();
            (status, body).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            axum::Json(serde_json::json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_orchestrator_command(
    cmd: &str,
    state: Arc<crate::AppState>,
) -> axum::response::Response {
    let trimmed = cmd.trim_start_matches('/');
    let (action, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));

    match action {
        "research" => {
            let goal = crate::orchestrator::OrchestratorGoal::deep_research(rest);
            dispatch_goal(goal, state).await
        }
        "code" | "implement" => {
            let files: Vec<String> = vec![]; // would come from args
            let goal = crate::orchestrator::OrchestratorGoal::coding_task(rest, files);
            dispatch_goal(goal, state).await
        }
        "review" => {
            let goal = crate::orchestrator::OrchestratorGoal::review_task(rest);
            dispatch_goal(goal, state).await
        }
        "orchestrate" | "plan" => {
            let goal = crate::orchestrator::OrchestratorGoal::coding_task(rest, vec![]);
            dispatch_goal(goal, state).await
        }
        "register" | "checkin" => {
            let _reg: crate::orchestrator::AgentRegistration = serde_json::from_str(rest).unwrap_or(
                crate::orchestrator::AgentRegistration {
                    id: rest.to_string(),
                    name: rest.to_string(),
                    provider: "unknown".into(),
                    protocol: crate::orchestrator::AgentProtocol::A2A,
                    capabilities: vec!["chat".into()],
                    connected_at: chrono::Utc::now().to_rfc3339(),
                    last_heartbeat: chrono::Utc::now().to_rfc3339(),
                }
            );
            (axum::http::StatusCode::OK,
             axum::Json(serde_json::json!({ "status": "registered", "agent_id": rest }))
            ).into_response()
        }
        "workflows" => {
            let workflows = vec![
                serde_json::json!({"id": "deep-research", "name": "Deep Research"}),
                serde_json::json!({"id": "coding", "name": "Coding Task"}),
                serde_json::json!({"id": "review", "name": "Code Review"}),
            ];
            (axum::http::StatusCode::OK,
             axum::Json(serde_json::json!({ "workflows": workflows }))
            ).into_response()
        }
        _ => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": format!("unknown command: {action}") })),
        )
            .into_response(),
    }
}

async fn dispatch_goal(
    goal: crate::orchestrator::OrchestratorGoal,
    state: Arc<crate::AppState>,
) -> axum::response::Response {
    let (tx, mut rx) = crate::orchestrator::create_event_channel();
    let engine = crate::orchestrator::FanOutEngine::new(tx);
    let workflow = goal.workflow.clone();
    let n_subtasks = goal.subtasks.len();

    tokio::spawn(async move {
        let _results = engine.execute(goal).await;
        while let Some(event) = rx.recv().await {
            let mut mem = state.pkg_ctx_memory.lock().await;
            let summary = format!("{:?}", event);
            let tokens = (summary.len() / 4) as i64;
            mem.insert(pkg_ctx::storage::DocChunk {
                id: 0,
                doc_path: "orchestrator".into(),
                doc_title: "orchestrator".into(),
                section_title: "event".into(),
                content: summary,
                tokens,
                has_code: false,
            }).await.ok();
        }
    });

    (axum::http::StatusCode::ACCEPTED,
     axum::Json(serde_json::json!({
        "status": "orchestrating",
        "workflow": workflow,
        "subtasks": n_subtasks,
    }))
    ).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_serde() {
        let json = r#"{"messages":[{"role":"user","content":"hello"}]}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn test_tool_call_serde() {
        let json = r#"{"id":"call-1","tool_name":"get_docs","arguments":{"library":"test"}}"#;
        let tc: ToolCall = serde_json::from_str(json).unwrap();
        assert_eq!(tc.tool_name, "get_docs");
    }
}
