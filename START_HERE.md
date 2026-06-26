# Portail — Start Here

Everything you need to know in one page.

---

## What is Portail?

Portail is a self-hosted proxy/gateway for AI services. It sits between your applications and AI providers (OpenAI, Anthropic, local models) and adds:

- **Caching** — Don't pay twice for the same prompt
- **Hooks** — Inject system prompts automatically
- **Tracing** — See every request end-to-end
- **DNS** — Resolve internal services securely
- **URL Shortening** — Share internal links easily
- **Agent Protocol** — A2A and A2C for multi-agent systems

---

## Network Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                        Your Application                         │
│   (Claude Code, Whale, OpenCode, custom app)                    │
└───────────────────────────────┬─────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Portail (Layer 4)                        │
│                                                                 │
│   ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐   │
│   │  Proxy    │  │  Cache    │  │  Hooks    │  │  Tracer   │   │
│   │  Routes   │  │  Redis +  │  │  Inject   │  │  E2E      │   │
│   │  requests │  │  Moka     │  │  prompts  │  │  traces   │   │
│   └───────────┘  └───────────┘  └───────────┘  └───────────┘   │
│                                                                 │
│   ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐   │
│   │  DNS      │  │  TinyURL  │  │  Events   │  │  Sentinel │   │
│   │  DoH +    │  │  Auto     │  │  Ring     │  │  Health   │   │
│   │  Unbound  │  │  shorten  │  │  buffer   │  │  checks   │   │
│   └───────────┘  └───────────┘  └───────────┘  └───────────┘   │
│                                                                 │
└───────────────────────────────┬─────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                     AI Providers (Layer 5)                       │
│                                                                 │
│   ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐   │
│   │  OpenAI   │  │  Anthropic│  │  LiteLLM  │  │  Local    │   │
│   │  GPT-4    │  │  Claude   │  │  Proxy    │  │  Ollama   │   │
│   └───────────┘  └───────────┘  └───────────┘  └───────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## OSI Model Quick Reference

| Layer | Name | What | Portail Uses |
|-------|------|------|--------------|
| 7 | Application | HTTP, DNS, TLS | axum, reqwest, DoH |
| 6 | Presentation | Encryption, encoding | TLS, JSON, base62 |
| 5 | Session | Connections | HTTP/2 keep-alive |
| 4 | Transport | TCP, UDP | tokio TCP |
| 3 | Network | IP routing | IP headers |
| 2 | Data Link | Ethernet | N/A |
| 1 | Physical | Cables | N/A |

Portail operates at **Layer 7** (Application) and **Layer 4** (Transport).

---

## Request Flow

```
Client                    Portail                    Upstream
  │                          │                          │
  │  POST /v1/chat           │                          │
  │─────────────────────────>│                          │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ Check   │                     │
  │                     │ Cache   │                     │
  │                     └────┬────┘                     │
  │                          │ miss                     │
  │                     ┌────┴────┐                     │
  │                     │ Apply   │                     │
  │                     │ Hooks   │                     │
  │                     └────┬────┘                     │
  │                          │                          │
  │                          │  POST /v1/chat           │
  │                          │─────────────────────────>│
  │                          │                          │
  │                          │  200 OK                  │
  │                          │<─────────────────────────│
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ Store   │                     │
  │                     │ Cache   │                     │
  │                     └────┬────┘                     │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ Record  │                     │
  │                     │ Trace   │                     │
  │                     └────┬────┘                     │
  │                          │                          │
  │  200 OK                  │                          │
  │<─────────────────────────│                          │
  │                          │                          │
```

---

## DNS Explained

```
What happens when you type "example.com":

1. Browser asks: "What IP is example.com?"
2. OS checks local cache → miss
3. OS asks resolver (1.1.1.1 or 8.8.8.8)
4. Resolver asks root servers: "Who handles .com?"
5. Root says: "Ask the .com TLD servers"
6. Resolver asks TLD: "Who handles example.com?"
7. TLD says: "Ask ns1.example.com"
8. Resolver asks authoritative: "What IP for example.com?"
9. Authoritative says: "93.184.216.34"
10. Browser connects to 93.184.216.34

Portail adds:
- DNS over HTTPS (DoH) — encrypts step 3
- DNS hooks — can block/redirect at step 2
- Network isolation — controls which domains are allowed
```

---

## TLS/SSL Explained

```
How HTTPS works:

Client                              Server
  │                                    │
  │  "Hello, I support TLS 1.3"       │
  │───────────────────────────────────>│
  │                                    │
  │  "Hello, here's my certificate"   │
  │<───────────────────────────────────│
  │                                    │
  │  [Verify certificate]             │
  │  [Generate session key]           │
  │                                    │
  │  "Let's use this key"             │
  │───────────────────────────────────>│
  │                                    │
  │  [Encrypted communication]        │
  │<──────────────────────────────────>│

Portail supports:
- Self-signed (development)
- Let's Encrypt (production)
- Custom certificates
```

---

## TCP/UDP Quick Guide

```
TCP (Transmission Control Protocol):
- Reliable, ordered delivery
- Three-way handshake: SYN → SYN-ACK → ACK
- Used for: HTTP, SSH, database connections
- Portail uses TCP for all HTTP traffic

UDP (User Datagram Protocol):
- Fast, no guarantees
- No handshake, just send
- Used for: DNS, video streaming, gaming
- Portail uses UDP for DNS queries
```

---

## Portail Components

### Core
1. **Proxy (axum)** — Routes incoming requests to the right handler
2. **Cache (Redis + Moka)** — Two tiers: in-memory + network-wide
3. **Hooks** — Inject system prompts into AI requests automatically
4. **Tracer** — Records every request with timing for each step
5. **DNS** — Resolves internal services with DoH for privacy
6. **TinyURL** — Shortens internal URLs for easy sharing
7. **Events** — Ring buffer of agent lifecycle events with SSE streaming
8. **Sentinel** — Background health checker, publishes status every 30s

### Agents
9. **A2A** — Agent-to-Agent protocol for multi-agent systems
10. **A2C** — Agent-to-Consumer chat interface
11. **NullClaw** — Network-native heartbeat agent
12. **Godfather** — Internal service monitor
13. **Discovery** — Self-service network discovery

### Performance Engines
14. **eBPF** — Kernel-level observability (tracing syscalls, network latency)
15. **io_uring** — Async I/O engine (Linux 5.1+, reduces syscall overhead)
16. **DPDK** — Kernel bypass for extreme performance (requires dedicated NIC)
17. **Hyper** — Low-level HTTP engine (direct hyper control, skip axum overhead)

---

## Configuration

Minimal `portail.toml`:

```toml
listen = "0.0.0.0:8787"

[ai_gateway]
enabled = true
upstream = "http://127.0.0.1:4000"

[redis]
enabled = true
url = "redis://127.0.0.1:6379"
max_memory_mb = 2048

[tinyurl]
enabled = true
base_url = "http://localhost:8787"
```

---

## CLI Commands

```bash
# Start server
portail serve

# Interactive dashboard
portail

# Check status
portail status

# View events
portail events

# Manage hooks
portail hooks list
portail hooks add --hook '{"id":"h1","match_path":"/chat","inject":"prepend","content":"Be helpful"}'

# Learn networking
portail learn dns
portail learn tls
portail learn tcp

# Setup domain + certificates
portail setup --domain portail.example.com

# Analyze code complexity
portail complexity

# Generate docs
portail docs --open
```

---

## Integration Examples

### Claude Code

```json
// .claude/settings.json
{
  "env": {
    "OPENAI_BASE_URL": "http://localhost:8787/v1"
  }
}
```

### OpenCode

```json
// opencode.json
{
  "mcp": {
    "servers": {
      "portail": {
        "command": "portail",
        "args": ["serve"]
      }
    }
  }
}
```

### Whale

```yaml
# whale.yaml
plugins:
  portail:
    url: http://localhost:8787
```

---

## Installation

```bash
# Quick install
curl -fsSL https://raw.githubusercontent.com/peterlodri-sec/portail/main/scripts/install.sh | bash

# Cargo
cargo install portail

# Nix
nix profile install github:peterlodri-sec/portail

# Docker
docker run -p 8787:8787 ghcr.io/peterlodri-sec/portail:latest
```

---

## File Structure

```
portail/
├── src/
│   ├── main.rs          # Entry point, CLI dispatch
│   ├── lib.rs           # AppState, module declarations
│   ├── proxy.rs         # HTTP routing
│   ├── gateway.rs       # AI upstream forwarding
│   ├── cdn.rs           # Cache (Moka + disk)
│   ├── events.rs        # Event log + SSE
│   ├── hooks.rs         # Prompt injection
│   ├── sentinel.rs      # Health monitoring
│   ├── dns.rs           # DNS + DoH
│   ├── a2a.rs           # Agent-to-Agent
│   ├── a2c.rs           # Agent-to-Consumer
│   ├── mcp.rs           # MCP sidecar proxy
│   ├── cli/
│   │   ├── mod.rs       # CLI types
│   │   ├── dashboard.rs # TUI dashboard
│   │   ├── complexity.rs # Code analysis
│   │   ├── install.rs   # Installer
│   │   ├── learn.rs     # Network guide
│   │   └── setup.rs     # Domain + certs
│   └── plugins/
│       ├── mod.rs       # Plugin declarations
│       ├── tinyurl.rs   # URL shortening
│       ├── tracer.rs    # Request tracing
│       └── redis_cache.rs # App-level cache
├── nix/                 # NixOS modules
├── packaging/           # deb, rpm, snap, flatpak
├── scripts/             # Shell scripts
└── tests/               # Integration tests
```

---

## Next Steps

1. **Install**: `cargo install portail`
2. **Run**: `portail serve`
3. **Test**: `curl http://localhost:8787/healthz`
4. **Configure**: Edit `portail.toml`
5. **Learn**: `portail learn dns`
6. **Setup**: `portail setup --domain your-domain.com`

---

## Resources

- **GitHub**: https://github.com/peterlodri-sec/portail
- **Crates.io**: https://crates.io/crates/portail
- **Docs**: `portail docs --open`
- **Learn**: `portail learn <topic>`

---

*One page. Everything explained. Start building.*
