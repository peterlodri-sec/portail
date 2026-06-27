use crate::PKG_DIR;
use crate::search::{DocSearch, format_search_results};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::path::Path;

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

pub async fn serve_stdio(pkg_dir: Option<&Path>) -> Result<()> {
    let dir = pkg_dir.unwrap_or_else(|| Path::new(PKG_DIR));
    std::fs::create_dir_all(dir)?;
    let search = DocSearch::new(dir);

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();

    let mut buf = String::new();
    let mut initialized = false;

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                send(
                    &mut stdout,
                    None,
                    Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                    }),
                    None,
                )?;
                continue;
            }
        };

        match req.method.as_str() {
            "initialize" => {
                initialized = true;
                send(
                    &mut stdout,
                    req.id,
                    None,
                    Some(serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "pkg-ctx", "version": "0.1.0" }
                    })),
                )?;
            }
            "tools/list" => {
                let packages = search.list_installed().unwrap_or_default();
                let description = if packages.is_empty() {
                    "Search installed documentation packages. Install packages with `portail pkg-ctx add <repo>`.".to_string()
                } else {
                    format!(
                        "Search installed documentation packages. Available: {}.",
                        packages.join(", ")
                    )
                };

                send(
                    &mut stdout,
                    req.id,
                    None,
                    Some(serde_json::json!({
                        "tools": [{
                            "name": "get_docs",
                            "description": description,
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "library": {
                                        "type": "string",
                                        "description": "Package name with optional version (e.g. next.js@latest)"
                                    },
                                    "topic": {
                                        "type": "string",
                                        "description": "Search query"
                                    }
                                },
                                "required": ["library", "topic"]
                            }
                        }]
                    })),
                )?;
            }
            "tools/call" => {
                if !initialized {
                    send(
                        &mut stdout,
                        req.id,
                        Some(JsonRpcError {
                            code: -32000,
                            message: "Not initialized".into(),
                        }),
                        None,
                    )?;
                    continue;
                }
                let result = handle_tool_call(&search, &req.params).unwrap_or_else(|e| {
                    serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {e}")
                        }]
                    })
                });
                send(&mut stdout, req.id, None, Some(result))?;
            }
            "notifications/initialized" => {
                continue;
            }
            _ => {
                send(
                    &mut stdout,
                    req.id,
                    Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", req.method),
                    }),
                    None,
                )?;
            }
        }
    }
    Ok(())
}

fn handle_tool_call(search: &DocSearch, params: &Value) -> Result<Value> {
    let args = params
        .get("arguments")
        .ok_or_else(|| anyhow::anyhow!("missing arguments"))?;

    let library = args
        .get("library")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing library argument"))?;

    let topic = args
        .get("topic")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing topic argument"))?;

    let results = search.search_package(library, topic, 10)?;
    let text = format_search_results(&results, library, topic);

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": text
        }]
    }))
}

fn send(
    stdout: &mut impl Write,
    id: Option<Value>,
    error: Option<JsonRpcError>,
    result: Option<Value>,
) -> Result<()> {
    let response = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result,
        error,
    };
    let json = serde_json::to_string(&response)?;
    writeln!(stdout, "{json}")?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialized_check() {
        let req: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
                .unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(Value::Number(1.into())));
    }
}
