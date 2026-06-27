//! Unix socket MCP server — same binary framing protocol as the Python sidecar.
//!
//! This replaces `start_sidecar()` / `proxy_to_sidecar()` from src/mcp/mod.rs
//! but keeps the exact same wire format so the gateway doesn't change.

use bytes::{BufMut, BytesMut};
use metrics::counter;
use std::collections::HashMap;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

use crate::plugin::McpWasmPlugin;

const MAX_BODY_BYTES: usize = 10_000_000;

/// WASM-based MCP server that listens on a Unix socket
pub struct WasmMcpServer {
    socket_path: String,
    #[allow(dead_code)]
    plugins: Vec<McpWasmPlugin>,
}

impl WasmMcpServer {
    pub fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
            plugins: Vec::new(),
        }
    }

    /// Register a WASM MCP plugin
    pub fn add_plugin(&mut self, plugin: McpWasmPlugin) {
        info!(name = plugin.name(), "Registering WASM MCP plugin");
        self.plugins.push(plugin);
    }

    /// Start the Unix socket server
    pub async fn serve(&self) -> anyhow::Result<()> {
        let sock = Path::new(&self.socket_path);
        if let Some(parent) = sock.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let _ = tokio::fs::remove_file(&self.socket_path).await;

        let listener = UnixListener::bind(&self.socket_path)?;
        info!(path = %self.socket_path, "WASM MCP server listening");

        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(&mut stream).await {
                            debug!(error = %e, "MCP connection error");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept MCP connection");
                }
            }
        }
    }
}

/// Handle a single Unix socket connection using the binary framing protocol
async fn handle_connection(stream: &mut tokio::net::UnixStream) -> anyhow::Result<()> {
    // Read the request frame: [method_len:u16][method][path_len:u32][path][headers_len:u32][headers][body_len:u64][body]
    let mut method_len_buf = [0u8; 2];
    stream.read_exact(&mut method_len_buf).await?;
    let method_len = u16::from_be_bytes(method_len_buf) as usize;

    let mut method_buf = vec![0u8; method_len];
    stream.read_exact(&mut method_buf).await?;
    let method = String::from_utf8(method_buf)?;

    let mut path_len_buf = [0u8; 4];
    stream.read_exact(&mut path_len_buf).await?;
    let path_len = u32::from_be_bytes(path_len_buf) as usize;

    let mut path_buf = vec![0u8; path_len];
    stream.read_exact(&mut path_buf).await?;
    let path = String::from_utf8(path_buf)?;

    let mut headers_len_buf = [0u8; 4];
    stream.read_exact(&mut headers_len_buf).await?;
    let headers_len = u32::from_be_bytes(headers_len_buf) as usize;

    let mut headers_buf = vec![0u8; headers_len];
    stream.read_exact(&mut headers_buf).await?;
    let _headers: HashMap<String, String> = serde_json::from_slice(&headers_buf)?;

    let mut body_len_buf = [0u8; 8];
    stream.read_exact(&mut body_len_buf).await?;
    let body_len = u64::from_be_bytes(body_len_buf) as usize;

    if body_len > MAX_BODY_BYTES {
        warn!(body_len, "MCP request body too large");
        send_error_response(stream, 413, "Request body too large").await?;
        return Ok(());
    }

    let mut body_buf = vec![0u8; body_len];
    stream.read_exact(&mut body_buf).await?;

    counter!("mcp_wasm_requests").increment(1);
    debug!(method, path, body_len, "MCP request received");

    // Parse the body as JSON-RPC
    let request: serde_json::Value = match serde_json::from_slice(&body_buf) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Invalid JSON-RPC request");
            send_jsonrpc_error(stream, -32700, "Parse error").await?;
            return Ok(());
        }
    };

    // Route to the appropriate handler based on path
    let response = route_request(&request).await;

    // Send the response using the binary framing protocol
    send_json_response(stream, &response).await?;

    Ok(())
}

/// Route an MCP JSON-RPC request to the appropriate handler
async fn route_request(request: &serde_json::Value) -> serde_json::Value {
    let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = request.get("id");

    match method {
        "initialize" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false }
                    },
                    "serverInfo": {
                        "name": "portail-wasm-mcp",
                        "version": "0.1.0"
                    }
                }
            })
        }
        "tools/list" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "portail_health",
                            "description": "Check portail health status",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "portail_status",
                            "description": "Get portail server status",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        }
                    ]
                }
            })
        }
        "tools/call" => {
            let tool_name = request
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");

            match tool_name {
                "portail_health" => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{
                            "type": "text",
                            "text": "{\"status\": \"healthy\", \"runtime\": \"wasm\"}"
                        }]
                    }
                }),
                "portail_status" => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{
                            "type": "text",
                            "text": "{\"server\": \"portail\", \"mcp\": \"wasm\", \"version\": \"0.1.0\"}"
                        }]
                    }
                }),
                _ => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Unknown tool: {}", tool_name)
                    }
                }),
            }
        }
        "ping" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        }),
        _ => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {}", method)
            }
        }),
    }
}

/// Send a JSON response using the binary framing protocol
async fn send_json_response(
    stream: &mut tokio::net::UnixStream,
    response: &serde_json::Value,
) -> anyhow::Result<()> {
    let body = serde_json::to_vec(response)?;
    let headers = serde_json::to_string(&HashMap::<String, String>::new())?;

    let mut buf = BytesMut::with_capacity(6 + headers.len() + 8 + body.len());
    buf.put_u16(200);
    buf.put_u32(headers.len() as u32);
    buf.put_slice(headers.as_bytes());
    buf.put_u64(body.len() as u64);
    buf.put_slice(&body);

    stream.write_all(&buf).await?;
    stream.shutdown().await?;
    Ok(())
}

/// Send an error response
async fn send_error_response(
    stream: &mut tokio::net::UnixStream,
    _status: u16,
    message: &str,
) -> anyhow::Result<()> {
    let body = serde_json::json!({"error": message});
    send_json_response(stream, &body).await
}

/// Send a JSON-RPC error response
async fn send_jsonrpc_error(
    stream: &mut tokio::net::UnixStream,
    code: i32,
    message: &str,
) -> anyhow::Result<()> {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": {
            "code": code,
            "message": message
        }
    });
    send_json_response(stream, &response).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn route_initialize() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.1.0"}
            }
        });
        let response = route_request(&request).await;
        assert_eq!(response["result"]["serverInfo"]["name"], "portail-wasm-mcp");
    }

    #[tokio::test]
    async fn route_tools_list() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });
        let response = route_request(&request).await;
        assert!(response["result"]["tools"].is_array());
    }

    #[tokio::test]
    async fn route_tools_call_health() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "portail_health"}
        });
        let response = route_request(&request).await;
        assert!(
            response["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("healthy")
        );
    }

    #[tokio::test]
    async fn route_unknown_method() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "bogus"
        });
        let response = route_request(&request).await;
        assert_eq!(response["error"]["code"], -32601);
    }
}
