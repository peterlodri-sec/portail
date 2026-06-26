use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::{BufMut, Bytes, BytesMut};
use metrics::counter;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;
use tracing::{debug, info, warn};

const SOCKET_TIMEOUT_SECS: u64 = 30;
const MAX_BODY_BYTES: usize = 10_000_000;

pub async fn start_sidecar(socket_path: &str) -> anyhow::Result<()> {
    let sock = Path::new(socket_path);
    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _ = tokio::fs::remove_file(socket_path).await;

    let child = Command::new("uv")
        .args([
            "run",
            "--with",
            "portail-mcp",
            "python",
            "-m",
            "portail_mcp.server",
            "--socket",
            socket_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start MCP sidecar: {e}"))?;

    info!(pid = child.id().unwrap_or(0), %socket_path, "MCP sidecar started");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    if !sock.exists() {
        warn!("MCP sidecar socket not yet created — will retry on first request");
    }

    if let Some(stdout) = child.stdout {
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stdout);
            let mut buf = String::new();
            while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
                debug!("[mcp-sidecar] {}", buf.trim());
                buf.clear();
            }
        });
    }
    if let Some(stderr) = child.stderr {
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut buf = String::new();
            while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
                debug!("[mcp-sidecar:err] {}", buf.trim());
                buf.clear();
            }
        });
    }
    Ok(())
}

pub async fn proxy_to_sidecar(socket_path: &str, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let path = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let method = parts.method.to_string();
    let body_bytes = axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .unwrap_or_default();
    let socket_path = socket_path.to_string();

    match tokio::time::timeout(
        std::time::Duration::from_secs(SOCKET_TIMEOUT_SECS),
        proxy_via_unix(&socket_path, &method, path, &parts.headers, &body_bytes),
    )
    .await
    {
        Ok(Ok(response)) => {
            counter!("mcp_requests", "status" => "success").increment(1);
            response
        }
        Ok(Err(e)) => {
            warn!(%socket_path, error = %e, "MCP proxy error");
            counter!("mcp_requests", "status" => "error").increment(1);
            (StatusCode::BAD_GATEWAY, format!("MCP proxy error: {e}")).into_response()
        }
        Err(_) => {
            warn!(%socket_path, "MCP sidecar timed out");
            counter!("mcp_requests", "status" => "timeout").increment(1);
            (StatusCode::GATEWAY_TIMEOUT, "MCP sidecar timed out").into_response()
        }
    }
}

async fn proxy_via_unix(
    socket_path: &str,
    method: &str,
    path: &str,
    headers: &axum::http::HeaderMap,
    body: &[u8],
) -> anyhow::Result<Response> {
    let mut stream = UnixStream::connect(socket_path).await?;
    let frame = encode_frame(method, path, headers_to_map(headers), body);
    stream.write_all(&frame).await?;
    stream.shutdown().await?;
    decode_response(&mut stream).await
}

pub fn encode_frame(
    method: &str,
    path: &str,
    headers: HashMap<String, String>,
    body: &[u8],
) -> BytesMut {
    let headers_json = serde_json::to_string(&headers).unwrap_or_default();
    let method = method.as_bytes();
    let path = path.as_bytes();
    let headers = headers_json.as_bytes();
    let mut buf = BytesMut::with_capacity(
        2 + method.len() + 4 + path.len() + 4 + headers.len() + 8 + body.len(),
    );

    buf.put_u16(method.len() as u16);
    buf.put_slice(method);
    buf.put_u32(path.len() as u32);
    buf.put_slice(path);
    buf.put_u32(headers.len() as u32);
    buf.put_slice(headers);
    buf.put_u64(body.len() as u64);
    buf.put_slice(body);
    buf
}

async fn decode_response(stream: &mut UnixStream) -> anyhow::Result<Response> {
    let mut status_hdr = [0u8; 6];
    stream.read_exact(&mut status_hdr).await?;
    let status_code = u16::from_be_bytes([status_hdr[0], status_hdr[1]]);
    let headers_len =
        u32::from_be_bytes([status_hdr[2], status_hdr[3], status_hdr[4], status_hdr[5]]) as usize;

    let mut headers_buf = vec![0u8; headers_len];
    stream.read_exact(&mut headers_buf).await?;
    let resp_headers: HashMap<String, String> = serde_json::from_slice(&headers_buf)?;

    let mut body_len_buf = [0u8; 8];
    stream.read_exact(&mut body_len_buf).await?;
    let body_len = u64::from_be_bytes(body_len_buf) as usize;

    let mut resp_body = vec![0u8; body_len];
    stream.read_exact(&mut resp_body).await?;

    let mut out_headers = axum::http::HeaderMap::new();
    for (k, v) in resp_headers {
        if let (Ok(name), Ok(val)) = (
            axum::http::HeaderName::from_bytes(k.as_bytes()),
            axum::http::HeaderValue::from_str(&v),
        ) {
            out_headers.insert(name, val);
        }
    }

    Ok((
        axum::http::StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
        out_headers,
        Bytes::from(resp_body),
    )
        .into_response())
}

fn headers_to_map(headers: &axum::http::HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            map.insert(k.to_string(), val.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "text/plain".into());
        let frame = encode_frame("POST", "/mcp/tools/call", headers, b"hello");
        assert!(frame.len() > 10);

        let method_len = u16::from_be_bytes([frame[0], frame[1]]);
        let path_start = 2 + method_len as usize;
        let path_len = u32::from_be_bytes([
            frame[path_start],
            frame[path_start + 1],
            frame[path_start + 2],
            frame[path_start + 3],
        ]);
        let headers_start = path_start + 4 + path_len as usize;
        let headers_len = u32::from_be_bytes([
            frame[headers_start],
            frame[headers_start + 1],
            frame[headers_start + 2],
            frame[headers_start + 3],
        ]);
        let body_start = headers_start + 4 + headers_len as usize;
        let body_len = u64::from_be_bytes([
            frame[body_start],
            frame[body_start + 1],
            frame[body_start + 2],
            frame[body_start + 3],
            frame[body_start + 4],
            frame[body_start + 5],
            frame[body_start + 6],
            frame[body_start + 7],
        ]);

        let method_str = std::str::from_utf8(&frame[2..path_start]).unwrap();
        assert_eq!(method_str, "POST");
        let body_bytes = &frame[body_start + 8..body_start + 8 + body_len as usize];
        assert_eq!(body_bytes, b"hello");
    }

    #[test]
    fn headers_to_map_empty() {
        let headers = axum::http::HeaderMap::new();
        let map = headers_to_map(&headers);
        assert!(map.is_empty());
    }

    #[test]
    fn headers_to_map_with_values() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-foo", "bar".parse().unwrap());
        let map = headers_to_map(&headers);
        assert_eq!(map.get("x-foo").unwrap(), "bar");
    }
}
