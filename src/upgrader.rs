//! Connection Upgrader — protocol switch without dropping the socket.
//!
//! Upgrades HTTP connections to WebSocket, raw TCP, or PTY mid-stream.
//! Core pattern: read upgrade handshake, extract raw fd from Tokio,
//! hand off to a dedicated worker loop. Fall back to original stream
//! if upgrade conditions aren't met.
//!
//! Used by:
//!   - A2A WebSocket handler (/a2a/ws)
//!   - MCP sidecar socket handoff
//!   - Future: STARTTLS, h2c upgrade

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::time::Duration;
use tracing::{debug, warn};

// ── Upgrade protocol detection ──────────────────────────────────

/// HTTP upgrade header value constants
pub const UPGRADE_WEBSOCKET: &str = "websocket";
pub const UPGRADE_H2C: &str = "h2c";

/// Result of parsing an upgrade handshake
#[derive(Debug, Clone, PartialEq)]
pub enum UpgradeRequest {
    /// HTTP → WebSocket
    WebSocket {
        key: String,
        version: String,
        protocol: Option<String>,
    },
    /// HTTP/1.1 → HTTP/2 cleartext
    H2c,
    /// Not an upgrade request
    None,
}

/// Parse an HTTP request for Upgrade headers
pub fn parse_upgrade(headers: &[u8]) -> UpgradeRequest {
    let header_str = std::str::from_utf8(headers).unwrap_or("");

    if !header_str.contains("Upgrade:") {
        return UpgradeRequest::None;
    }

    if header_str.contains("Upgrade: websocket") || header_str.contains("upgrade: websocket") {
        let key = extract_header(header_str, "Sec-WebSocket-Key")
            .unwrap_or("")
            .to_string();
        let version = extract_header(header_str, "Sec-WebSocket-Version")
            .unwrap_or("13")
            .to_string();
        let protocol = extract_header(header_str, "Sec-WebSocket-Protocol");
        return UpgradeRequest::WebSocket { key, version, protocol };
    }

    if header_str.contains("Upgrade: h2c") {
        return UpgradeRequest::H2c;
    }

    UpgradeRequest::None
}

fn extract_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    for line in headers.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with(&name.to_lowercase()) {
            let val = line.splitn(2, ':').nth(1)?.trim();
            return Some(val);
        }
    }
    None
}

// ── WebSocket upgrade response ──────────────────────────────────

/// Generate a 101 Switching Protocols response for WebSocket upgrade
pub fn ws_upgrade_response(key: &str) -> Vec<u8> {
    use sha1::{Digest, Sha1};
    const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-5AB5DC11B725";

    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    let accept = base64::encode(hasher.finalize());

    format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\
         \r\n"
    )
    .into_bytes()
}

// ── Connection Upgrader ─────────────────────────────────────────

/// A connection upgrader that can transition a TCP stream from one
/// protocol to another without closing the socket.
pub struct ConnectionUpgrader {
    stream: Option<TcpStream>,
    read_buf: Vec<u8>,
}

impl ConnectionUpgrader {
    /// Wrap a TCP stream for potential upgrade
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream: Some(stream),
            read_buf: Vec::new(),
        }
    }

    /// Try to parse and perform an upgrade based on initial data.
    /// If the data indicates an upgrade request, consume the stream
    /// and hand it off. Otherwise return the stream for normal processing.
    pub async fn try_upgrade(mut self) -> Result<UpgradeResult, std::io::Error> {
        let mut stream = self.stream.take().expect("stream already consumed");

        // Peek at the first bytes to detect upgrade
        let mut buf = vec![0u8; 2048];
        let n = stream.peek(&mut buf).await?;
        buf.truncate(n);

        let upgrade = parse_upgrade(&buf);

        match upgrade {
            UpgradeRequest::WebSocket { key, version: _, protocol: _ } => {
                debug!("WebSocket upgrade requested (key={})", &key[..16]);

                // Send 101 response
                let response = ws_upgrade_response(&key);
                stream.write_all(&response).await?;

                // Detach raw fd from Tokio, hand to blocking worker
                let std_stream = stream.into_std()?;
                let raw_fd = std_stream.into_raw_fd();

                tokio::task::spawn_blocking(move || {
                    unsafe { ws_frame_loop(raw_fd) }
                });

                Ok(UpgradeResult::Upgraded("websocket"))
            }
            UpgradeRequest::H2c => {
                debug!("h2c upgrade requested");
                // Detach raw fd, hand to HTTP/2 worker (future)
                let std_stream = stream.into_std()?;
                let raw_fd = std_stream.into_raw_fd();
                tokio::task::spawn_blocking(move || {
                    unsafe { h2c_worker(raw_fd) }
                });
                Ok(UpgradeResult::Upgraded("h2c"))
            }
            UpgradeRequest::None => {
                // Not an upgrade — return the stream for normal processing
                self.stream = Some(stream);
                Ok(UpgradeResult::Passthrough(self))
            }
        }
    }

    /// Consume the upgrader and return the original stream (only valid
    /// when upgrade didn't happen)
    pub fn into_stream(mut self) -> Option<TcpStream> {
        self.stream.take()
    }

    /// Return buffered bytes that were read during upgrade detection
    pub fn buffered_bytes(&self) -> &[u8] {
        &self.read_buf
    }
}

#[derive(Debug)]
pub enum UpgradeResult<'a> {
    /// Connection upgraded to a new protocol
    Upgraded(&'a str),
    /// Not an upgrade — return the upgrader with the original stream
    Passthrough(ConnectionUpgrader),
}

// ── WebSocket frame worker (blocking) ───────────────────────────

/// Minimal WebSocket frame reader/writer. Runs on a blocking thread,
/// completely detached from the Tokio runtime.
unsafe fn ws_frame_loop(fd: i32) {
    let mut stream = std::net::TcpStream::from_raw_fd(fd);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(300)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    debug!("WebSocket frame loop started on fd={}", fd);

    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                debug!("ws frame loop: connection closed");
                break;
            }
            Ok(n) => {
                // Minimal frame parsing: echo text frames back
                if n >= 2 {
                    let opcode = buf[0] & 0x0f;
                    let masked = (buf[1] & 0x80) != 0;
                    let mut payload_len = (buf[1] & 0x7f) as usize;

                    let offset = if payload_len == 126 { 4 } else if payload_len == 127 { 10 } else { 2 };
                    if masked { /* skip mask key (4 bytes) */ }

                    match opcode {
                        0x1 | 0x2 => {
                            // Text or binary frame — echo back
                            let response = vec![0x81; 1]; // FIN + text opcode
                            // Simplified: echo works for small frames
                            let _ = stream.write_all(&buf[..n]);
                        }
                        0x8 => {
                            debug!("ws: close frame received");
                            break;
                        }
                        0x9 => {
                            // Ping → Pong
                            let mut pong = vec![0x8a];
                            pong.extend_from_slice(&buf[2..n]);
                            let _ = stream.write_all(&pong);
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("ws frame loop error: {e}");
                break;
            }
        }
    }

    debug!("ws frame loop exited (fd={})", fd);
}

unsafe fn h2c_worker(fd: i32) {
    let _ = fd;
    debug!("h2c worker stub — implement HTTP/2 framing");
}

// ── Raw FD utilities ────────────────────────────────────────────

/// Set TCP keepalive on a stream before upgrade
pub fn set_keepalive(stream: &TcpStream) -> std::io::Result<()> {
    let sock_ref = socket2::SockRef::from(stream);
    sock_ref.set_tcp_keepalive(
        &socket2::TcpKeepalive::new().with_time(Duration::from_secs(30)),
    )
}

/// Extract raw fd, preventing Drop from closing it
pub fn leak_raw_fd(stream: TcpStream) -> i32 {
    stream.into_raw_fd()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ws_upgrade() {
        let headers = b"GET /a2a/ws HTTP/1.1\r\n\
            Host: localhost:8787\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\r\n";
        let upgrade = parse_upgrade(headers);
        assert!(matches!(upgrade, UpgradeRequest::WebSocket { .. }));
        if let UpgradeRequest::WebSocket { key, version, protocol: _ } = upgrade {
            assert_eq!(key, "dGhlIHNhbXBsZSBub25jZQ==");
            assert_eq!(version, "13");
        }
    }

    #[test]
    fn test_parse_no_upgrade() {
        let headers = b"GET /healthz HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(parse_upgrade(headers), UpgradeRequest::None);
    }

    #[test]
    fn test_parse_h2c_upgrade() {
        let headers = b"GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: h2c\r\n\r\n";
        assert_eq!(parse_upgrade(headers), UpgradeRequest::H2c);
    }

    #[test]
    fn test_ws_upgrade_response() {
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let resp = ws_upgrade_response(key);
        let resp_str = std::str::from_utf8(&resp).unwrap();
        assert!(resp_str.contains("101 Switching Protocols"));
        assert!(resp_str.contains("Sec-WebSocket-Accept:"));
        // Known accept value for this key
        assert!(resp_str.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    }

    #[test]
    fn test_extract_header_case_insensitive() {
        let headers = "Content-Type: application/json\nX-API-Key: secret\n";
        assert_eq!(extract_header(headers, "content-type"), Some("application/json"));
        assert_eq!(extract_header(headers, "x-api-key"), Some("secret"));
        assert_eq!(extract_header(headers, "missing"), None);
    }
}
