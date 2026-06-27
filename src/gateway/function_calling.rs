//! Function Calling Router — intercept `tools` in AI requests, route to MCP sidecar.
//!
//! When a request contains `tools` (OpenAI function calling format), this module:
//! 1. Extracts tool definitions from the request body
//! 2. Routes tool calls to the MCP sidecar via Unix socket
//! 3. Injects tool results back into the conversation
//!
//! Supports OpenAI-compatible function calling format.

use crate::mcp;
use axum::http::request::Parts;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool definition in OpenAI format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

/// Tool call in assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Check if request body contains tools
pub fn has_tools(body: &serde_json::Value) -> bool {
    body.get("tools")
        .and_then(|v| v.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false)
}

/// Extract tool definitions from request body
pub fn extract_tools(body: &serde_json::Value) -> Vec<Tool> {
    body.get("tools")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Route tool calls to MCP sidecar and return results
pub async fn execute_tool_calls(
    socket_path: &str,
    tool_calls: &[ToolCall],
) -> anyhow::Result<Vec<ToolResult>> {
    let mut results = Vec::new();

    for call in tool_calls {
        let result = execute_single_tool(socket_path, call).await?;
        results.push(result);
    }

    Ok(results)
}

async fn execute_single_tool(socket_path: &str, call: &ToolCall) -> anyhow::Result<ToolResult> {
    // Build MCP request
    let mcp_request = serde_json::json!({
        "method": "tools/call",
        "params": {
            "name": call.function.name,
            "arguments": serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                .unwrap_or(serde_json::json!({}))
        }
    });

    let body_bytes = serde_json::to_vec(&mcp_request)?;
    let headers = HashMap::new();

    // Call MCP sidecar
    let frame = mcp::encode_frame("POST", "/mcp/tools/call", headers, &body_bytes);

    let mut stream = tokio::net::UnixStream::connect(socket_path).await?;
    tokio::io::AsyncWriteExt::write_all(&mut stream, &frame).await?;
    tokio::io::AsyncWriteExt::shutdown(&mut stream).await?;

    // Read response
    let mut status_buf = [0u8; 6];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut status_buf).await?;

    let headers_len =
        u32::from_be_bytes([status_buf[2], status_buf[3], status_buf[4], status_buf[5]]) as usize;
    let mut headers_buf = vec![0u8; headers_len];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut headers_buf).await?;

    let mut body_len_buf = [0u8; 8];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut body_len_buf).await?;
    let body_len = u64::from_be_bytes(body_len_buf) as usize;

    let mut resp_body = vec![0u8; body_len];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut resp_body).await?;

    let result: serde_json::Value = serde_json::from_slice(&resp_body)?;

    Ok(ToolResult {
        tool_call_id: call.id.clone(),
        content: result.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
}

/// Inject tool results back into messages
pub fn inject_tool_results(messages: &mut Vec<serde_json::Value>, tool_results: &[ToolResult]) {
    for result in tool_results {
        messages.push(serde_json::json!({
            "role": "tool",
            "tool_call_id": result.tool_call_id,
            "content": result.content
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_tools_true() {
        let body = serde_json::json!({
            "messages": [],
            "tools": [{"type": "function", "function": {"name": "test", "parameters": {}}}]
        });
        assert!(has_tools(&body));
    }

    #[test]
    fn has_tools_false() {
        let body = serde_json::json!({"messages": []});
        assert!(!has_tools(&body));
    }

    #[test]
    fn extract_tools_empty() {
        let body = serde_json::json!({"messages": []});
        let tools = extract_tools(&body);
        assert!(tools.is_empty());
    }
}
