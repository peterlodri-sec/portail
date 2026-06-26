use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Consumer-facing types ────────────────────────────────────────

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

// ── Tool use (MCP integration) ───────────────────────────────────

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

// ── HTTP handlers ────────────────────────────────────────────────

pub async fn handle_chat(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(req): axum::Json<ChatRequest>,
) -> axum::response::Response {
    let upstream = {
        let c = state.config.read().unwrap();
        c.ai_gateway.as_ref().filter(|g| g.enabled).map(|g| g.upstream.clone())
    };

    let Some(upstream) = upstream else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({
            "error": "AI gateway not configured"
        }))).into_response();
    };

    // Log the request
    state.event_log.publish(crate::events::AgentEvent {
        agent_id: "a2c".into(),
        event_type: "chat_request".into(),
        severity: "info".into(),
        timestamp: 0,
        metadata: rustc_hash::FxHashMap::from_iter([
            ("model".into(), req.model.clone().unwrap_or_default()),
            ("stream".into(), req.stream.to_string()),
            ("messages".into(), req.messages.len().to_string()),
        ]),
    });

    // Forward to upstream using the existing gateway
    let body = serde_json::to_vec(&req).unwrap_or_default();
    match crate::gateway::forward_with_url(&upstream, "/v1/chat/completions", &body).await {
        Ok(resp) => resp,
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({
            "error": format!("upstream error: {}", e)
        }))).into_response(),
    }
}

// ── Module-level router ──────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/a2c/chat", axum::routing::post(handle_chat))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_serde() {
        let req = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hello".into(),
                name: None,
            }],
            stream: false,
            model: Some("gpt-4".into()),
            temperature: Some(0.7),
            max_tokens: Some(100),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].content, "hello");
    }

    #[test]
    fn tool_call_serde() {
        let call = ToolCall {
            id: "call_123".into(),
            tool_name: "read_file".into(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_name, "read_file");
    }
}
