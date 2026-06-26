<p align="center">
  <img src="docs/logo.svg" width="200" alt="Portail Logo">
</p>

# Portail

**Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache + Agent protocol + DNS + Observability**

<p align="center">
  <a href="https://portail.vaked.dev"><img src="https://portail.vaked.dev/ci/badge" alt="CI Status"></a>
  <a href="https://github.com/peterlodri-sec/portail/actions"><img src="https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml/badge.svg" alt="GitHub CI"></a>
  <a href="https://crates.io/crates/portail"><img src="https://img.shields.io/crates/v/portail" alt="Crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="https://github.com/peterlodri-sec/portail/actions/workflows/release.yml"><img src="https://img.shields.io/badge/tests-174%20passing-brightgreen" alt="Tests"></a>
</p>

<p align="center">
  <a href="https://pocoo.vaked.dev">Blog</a> · <a href="https://github.com/peterlodri-sec">GitHub</a> · <a href="https://x.com/0xp3t3rl">X/Twitter</a> · <a href="https://patreon.com/vaked">Patreon</a> · <a href="https://chat.vaked.dev">Chat</a>
</p>

> **v1.4.0** · 174 tests · 0 warnings · 7 CI agents · MIT-licensed since 2026.

Portail is a high-performance, self-hosted proxy and gateway giving you a single
control plane for AI infrastructure. Built in Rust. Start in 5 minutes. Scale to fleet.

**One binary.** Proxy, cache, hooks, agents, DNS, observability. Everything.

## Features

- **AI Gateway** — Stream proxy to LiteLLM, OpenAI, Anthropic, Ollama
- **MCP Gateway** — Unix socket sidecar for MCP tool execution
- **CDN Cache** — Two-tier (Moka in-memory + blake3 disk) with mmap zero-copy reads
- **A2A Protocol** — Google Agent-to-Agent: agent cards, task lifecycle, WebSocket streaming
- **A2C Chat** — Human-facing chat API with tool use, streaming, tokens
- **Hook Injection** — Per-message/per-event prompt injection, CRUD API
- **Event System** — Ring buffer + broadcast + SSE + NATS bridge for distributed events
- **DNS** — DoH resolution, network isolation, TTL cache, fallback chain
- **Observability** — OTLP traces (gRPC), Prometheus metrics, /dashboard health snapshot
- **Security** — Rate limiting (token bucket, per-key/per-endpoint), JWT/API-key auth, HSTS
- **Self-healing** — Config file watcher, auto-reload, validation, version history + rollback
- **TUI Dashboard** — Live sparklines, cache ratios, config health, keyboard navigation
- **Event Store** — SQLite (WAL, retention) with pluggable Turso/libSQL backend
- **6 CI Agents** — Advisory-only: complexity, drift-detect, spec-verify, fuzz-route, chore-bot, clippy
- **Type Hardened** — BoundedMeta (max 16 entries, key≤128B, val≤512B) replaces FxHashMap on hot paths
- **Config Wizard** — `portail init` interactive generator, zero-config startup
- **GraphQL API** — Async-graphql schema, query events + publish mutations
- **Keyboards CLI** — Status, events, hooks, cache, health, config — all HTTP-connected to running server

## Quick Start

```bash
# Install
cargo install portail

# Generate config (optional — works without it)
portail init

# Start server (zero config)
portail serve

# Interactive dashboard
portail

# Check health
portail health
curl http://localhost:8787/dashboard | jq
```

## Installation

| Method | Command |
|--------|---------|
| Cargo | `cargo install portail` |
| Nix | `nix profile install github:peterlodri-sec/portail` |
| Docker | `docker run -p 8787:8787 ghcr.io/peterlodri-sec/portail:latest` |
| Quick script | `curl -fsSL https://raw.githubusercontent.com/peterlodri-sec/portail/main/scripts/install.sh \| bash` |
| From source | `git clone && cargo build --release` |

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/healthz` | GET | Health check |
| `/readyz` | GET | Readiness (dependencies) |
| `/dashboard` | GET | Health snapshot: config, rate, auth, CDN |
| `/metrics` | GET | Prometheus metrics |
| `/v1/chat/completions` | POST | OpenAI-compatible chat |
| `/v1/messages` | POST | Anthropic-compatible messages |
| `/.well-known/agent.json` | GET | A2A agent card |
| `/a2a/tasks` | POST | Create A2A task |
| `/a2a/tasks/{id}` | GET | Get A2A task |
| `/a2a/ws` | WebSocket | A2A real-time streaming |
| `/a2c/chat` | POST | Agent-to-Consumer chat |
| `/events` | GET/POST | Recent events / publish |
| `/events/stream` | GET | SSE event stream |
| `/hooks` | GET/POST | List / create hooks |
| `/hooks/{id}` | DELETE | Delete hook |
| `/graphql` | POST/GET | GraphQL API |
| `/sessions` | GET | Session list |
| `/sessions/{id}` | GET | Session detail |
| `/supervisor/status` | GET | Background task status |
| `/file-cache/{key}` | PUT/GET/DELETE | Content-addressable file cache |
| `/file-cache/stats` | GET | File cache stats |
| `/cdn/{*path}` | * | CDN cache proxy |
| `/mcp/{*path}` | * | MCP sidecar proxy |

## CLI

```
portail                    # Interactive TUI dashboard
portail serve              # Start server (zero config)
portail init               # Interactive config wizard
portail status             # Version, config, server check
portail events             # Recent events (from running server)
portail hooks list         # List hooks
portail hooks add          # Add hook via JSON
portail health             # Health check
portail config show        # Show config (TOML)
portail config validate    # Validate config file
portail config rollback    # Rollback to previous version
portail complexity         # Big-O complexity analysis
portail docs               # Generate docs, open in browser
portail learn <topic>      # Learn networking concepts
portail setup              # Domain + TLS certificate setup
```

## Hardware Optimizations

- **mimalloc** — Global allocator, 2-3x alloc throughput
- **blake3** — Native SIMD (SSE2/AVX2/NEON) for cache keys
- **AHash** — DoS-resistant HashMap with per-process random seed
- **UPX** — Compressed release binary (<10MB)
- **LTO + PGO** — fat-LTO, codegen-units=1, strip symbols
- **io_uring** / **DPDK** — Feature-gated high-performance I/O engines

## CI & Agents

7 CI agents run on every push/PR:

| Agent | Blocks CI? | Status |
|-------|-----------|--------|
| complexity | ❌ advisory only | ✅ |
| drift-detect | ❌ advisory only | ✅ |
| spec-verify | ❌ advisory only | ✅ |
| fuzz-route | ⚠️ only on crash | ✅ |
| chore-bot | ❌ advisory only | ✅ |
| clippy | ✅ always | ✅ |
| test | ✅ always (174 passing) | ✅ |

All advisory agents post comments, never fail the build.

## Development

```bash
task c              # cargo check (fast)
task t              # cargo test (174 passing)
task lint           # clippy + fmt check
task counts         # test + warning counts
task chore-check    # auto-fixable issues
task bench          # criterion benchmarks
task docs           # generate + open docs
```

> **Explore**: [`AGENTS.md`](AGENTS.md) — codebase cross-reference hub.  
> **Product**: [`docs/architecture/PRODUCT.md`](docs/architecture/PRODUCT.md) — strategy + positioning.  
> **Architecture**: [`docs/architecture/DESIGN.md`](docs/architecture/DESIGN.md) + [`NETWORK_DESIGN.md`](docs/architecture/NETWORK_DESIGN.md).  
> **Roadmap**: [`LOOP_STATE.md`](LOOP_STATE.md) — current state + next milestones.  
> **Contribute**: [`docs/contributors/CONTRIBUTING.md`](docs/contributors/CONTRIBUTING.md).

## License

MIT
