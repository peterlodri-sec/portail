# Portail

**Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache + Agent-to-Agent protocol**

![CI Status](https://portail.vaked.dev/ci/badge)
[![GitHub CI](https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml/badge.svg)](https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/portail)](https://crates.io/crates/portail)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> **Live CI status** from portail.vaked.dev — updates automatically on every build.

Portail is a high-performance, self-hosted proxy and gateway for AI services, MCP tools, and CDN caching. Built in Rust with zero-copy I/O, SIMD-optimized hashing, and a live TUI dashboard.

## Features

- **AI Gateway** — Stream proxy to LiteLLM/OpenAI/Anthropic with hook injection
- **MCP Gateway** — Unix socket proxy to Python sidecar for tool execution
- **CDN Cache** — Two-tier (moka memory + blake3 filesystem) with NATS invalidation
- **A2A Protocol** — Google Agent-to-Agent: agent cards, task lifecycle, message streaming
- **A2C Interface** — Agent-to-Consumer: human-facing chat API with tool use
- **Hook Injection** — Per-message/per-event prompt injection with CRUD API
- **Event Log** — Ring buffer + broadcast channel for agent lifecycle events
- **Sentinel** — Background health watcher with auto-recovery
- **TUI Dashboard** — Live network visualization, sparklines, keyboard navigation

## Installation

### Quick Install (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/peterlodri-sec/portail/main/scripts/install.sh | bash
```

### Cargo Install

```bash
cargo install portail
```

### Nix/NixOS

```bash
# As a flake input
nix profile install github:peterlodri-sec/portail

# Or add to your flake.nix
inputs.portail.url = "github:peterlodri-sec/portail";
```

### Docker

```bash
docker run -p 8787:8787 ghcr.io/peterlodri-sec/portail:latest
```

### From Source

```bash
git clone https://github.com/peterlodri-sec/portail.git
cd portail
cargo build --release
sudo cp target/release/portail /usr/local/bin/
```

## Quick Start

```bash
# Install
cargo install portail

# Run with defaults (AI gateway on :8787)
portail serve

# Run with config file
portail serve --config portail.toml

# Launch interactive TUI dashboard
portail
```

## CLI

```
portail                    # Interactive TUI dashboard (default)
portail serve              # Start proxy server
portail status             # Show status
portail events             # Show recent events
portail hooks list         # List hooks
portail health             # Health check
portail config show        # Show configuration
```

## Configuration

```toml
# portail.toml
listen = "0.0.0.0:8787"

[ai_gateway]
enabled = true
upstream = "http://127.0.0.1:4000"

[mcp]
enabled = true
socket_path = "/run/portail/mcp.sock"

[cdn]
enabled = false
origin = "http://127.0.0.1:9000"
cache_dir = "/var/cache/portail"
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      portail binary                         │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  proxy    │  │  cdn     │  │  events  │  │  hooks   │   │
│  │  (axum)   │  │  (moka)  │  │  (ring)  │  │  (inject)│   │
│  └─────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│        │              │              │              │         │
│  ┌─────┴──────────────┴──────────────┴──────────────┴─────┐  │
│  │                   AppState (shared)                    │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ sentinel  │  │  mcp     │  │  a2a     │  │  a2c     │   │
│  │ (health)  │  │ (sidecar)│  │ (agent)  │  │ (chat)   │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/healthz` | GET | Health check |
| `/readyz` | GET | Readiness check |
| `/metrics` | GET | Prometheus metrics |
| `/v1/chat/completions` | POST | OpenAI-compatible chat |
| `/v1/messages` | POST | Anthropic-compatible messages |
| `/a2c/chat` | POST | Agent-to-Consumer chat |
| `/.well-known/agent.json` | GET | A2A agent card |
| `/a2a/tasks` | POST | Create A2A task |
| `/a2a/tasks/{id}` | GET | Get A2A task |
| `/events` | GET/POST | Recent events / publish |
| `/events/stream` | GET | SSE event stream |
| `/hooks` | GET/POST | List / create hooks |
| `/hooks/{id}` | DELETE | Delete hook |
| `/cdn/{*path}` | * | CDN cache proxy |
| `/mcp/{*path}` | * | MCP sidecar proxy |

## Hardware Optimizations

- **mimalloc** — Global allocator for 2-3x alloc throughput
- **rustc-hash** — FxHashMap for hot paths
- **blake3** — Native SIMD (SSE2/AVX2/NEON) for cache keys
- **LTO + UPX** — Release binary: fat-LTO + UPX compressed

## CI/Webhook

Portail has a built-in CI status webhook that shows live build status on the landing page.

### Setup (one-time)

```bash
./scripts/setup-webhook.sh
```

This will:
1. Generate a secure webhook secret
2. Store it in `~/.config/portail/webhook-secret`
3. Auto-configure GitHub webhook (if `gh` CLI available)

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/ci/status` | GET | JSON with workflow statuses |
| `/ci/badge` | GET | SVG badge for README |
| `/ci/live` | GET | SSE stream for live updates |
| `/ci/webhook` | POST | GitHub webhook receiver |

### Badge

```markdown
![CI](https://portail.vaked.dev/ci/badge)
```

## Development

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt

# Bench
cargo bench

# TUI dashboard (development)
cargo run
```

## License

MIT
