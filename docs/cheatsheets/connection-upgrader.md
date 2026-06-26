# Connection Upgrader — Protocol Switching in Rust

## What It Does

Upgrades a TCP connection from one protocol to another without closing
the socket: HTTP→WebSocket, TCP→TLS, stdin→PTY, single→multiplexed.

## Pattern 1: HTTP to WebSocket (Portail A2A/WS)

```rust
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::os::unix::io::{IntoRawFd, FromRawFd};

pub struct ConnectionUpgrader {
    stream: Option<TcpStream>,
}

impl ConnectionUpgrader {
    pub fn new(stream: TcpStream) -> Self {
        Self { stream: Some(stream) }
    }

    /// Read upgrade handshake, detach socket from HTTP runtime,
    /// hand off to a dedicated protocol loop.
    pub async fn upgrade_to_ws(mut self) -> Result<(), std::io::Error> {
        let mut stream = self.stream.take().unwrap();
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await?;
        let handshake = &buf[..n];

        if handshake.starts_with(b"GET /a2a/ws") && contains_upgrade_header(handshake) {
            // Send 101 Switching Protocols
            stream.write_all(b"HTTP/1.1 101 Switching Protocols\r\n\
                Upgrade: websocket\r\nConnection: Upgrade\r\n\r\n").await?;

            // Detach from Tokio runtime, hand to raw fd worker
            let std_stream = stream.into_std()?;
            let raw_fd = std_stream.into_raw_fd();
            tokio::task::spawn_blocking(move || {
                unsafe { ws_worker(raw_fd) }
            });
        }
        Ok(())
    }
}

unsafe fn ws_worker(fd: i32) {
    let mut stream = std::net::TcpStream::from_raw_fd(fd);
    // WebSocket frame loop: mask/unmask, read/write frames
}
```

## Pattern 2: Raw FD Extraction (Portail MCP Sidecar)

Used by Portail's MCP proxy to detach from axum and pass
the connection to the Python sidecar over a Unix socket.

```rust
use std::os::unix::io::AsRawFd;

// Extract fd for socket option manipulation
let fd = stream.as_raw_fd();

// Set TCP keepalive before upgrading
socket2::SockRef::from(&stream)
    .set_tcp_keepalive(&socket2::TcpKeepalive::new().with_time(std::time::Duration::from_secs(30)))?;

// Consume stream, take raw fd
let raw = stream.into_raw_fd(); // ownership transferred, no Drop
```

## Key APIS

| Trait | Function | Use |
|-------|----------|-----|
| `AsRawFd` | `fn as_raw_fd(&self) -> i32` | Borrow fd (set options) |
| `IntoRawFd` | `fn into_raw_fd(self) -> i32` | Consume, take ownership |
| `FromRawFd` | `unsafe fn from_raw_fd(fd: i32) -> Self` | Wrap back (unsafe!) |

## Upgrader Types

| Upgrade | Header / Signal | New Protocol |
|---------|----------------|--------------|
| HTTP→WebSocket | `Upgrade: websocket` | Full-duplex stream |
| HTTP/1.1→h2c | `Upgrade: h2c` | HTTP/2 multiplexed |
| TCP→TLS | `STARTTLS` | Encrypted session |
| Raw→PTY | `script` / `forkpty` | Interactive shell |
| Single→Mux | Custom binary protocol | SOCKS5 + shell + file xfer |

## Pitfalls

1. **Kernel buffer trap** — `BufReader` may have bytes in user-space that
   `IntoRawFd` leaves behind. Fix: pass remaining buffer alongside fd.
2. **Runtime de-registration** — Tokio's `into_std()` deregisters from
   epoll/kqueue. Bypassing this causes race conditions.
3. **State preservation** — Use `Option<TcpStream>.take()` so you can
   fall back to the original stream if upgrade fails.
