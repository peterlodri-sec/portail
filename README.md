<p align="center">
  <img src="docs/logo.svg" width="200" alt="Portail Logo">
</p>

<!-- [media_suggestion:::prompt:```A clean, modern hero banner for "Portail" — a Rust-based unified AI/MCP/CDN proxy. Wide 1600x500 image, dark navy background with neon-cyan and magenta accent lines forming a stylised network gateway (ports, packets, arrows). Minimal geometric style, subtle grid, the word "PORTAIL" centred in a bold sans-serif. No people, no logos.```] -->

# Portail

**Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache + Agent-to-Agent protocol**

<p align="center">
  <a href="https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml"><img src="https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/portail"><img src="https://img.shields.io/crates/v/portail" alt="Crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="https://github.com/peterlodri-sec/portail/blob/main/CONTRIBUTING.md"><img src="https://img.shields.io/badge/PRs-welcome-brightgreen.svg" alt="PRs welcome"></a>
</p>

<p align="center">
  <a href="START_HERE.md">Start Here</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="DESIGN.md">Design</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="SECURITY.md">Security</a>
</p>

> The optional live build badge from `portail.vaked.dev/ci/badge` is served by a
> self-hosted Portail instance (see [CI/Webhook](#ciwebhook)); enable it once
> you have your own deployment.

Portail is a high-performance, self-hosted proxy and gateway for AI services, MCP tools, and CDN caching. Built in Rust with zero-copy I/O, SIMD-optimized hashing, and a live TUI dashboard.

<!-- [media_suggestion:::prompt:```Animated terminal recording (GIF, ~10s, 900x500): a developer runs `portail serve`, the TUI dashboard appears with live sparklines for requests/sec, cache hit rate and AI gateway latency, then they press `q` to quit. Dark terminal, monospace font, cyan and green accent colors. No audio.```] -->

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

<!-- [media_suggestion:::prompt:```A high-resolution architectural diagram (1600x900, SVG-style flat illustration) showing Portail at the centre as a rounded rectangle, with four inbound channels on the left (HTTP client, MCP client, CDN edge, A2A agent) and four outbound channels on the right (OpenAI, Anthropic, LiteLLM, local Ollama). Inside the rectangle show six labelled blocks: Proxy (axum), Cache (moka+redis), Hooks, Events, Sentinel, Discovery. Dark navy background, neon-cyan strokes, white text. Minimal, technical, no people.```] -->

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

Once you have your own Portail deployment running with the webhook configured:

```markdown
![CI](https://<your-portail-host>/ci/badge)
```

The default GitHub Actions badge above (at the top of this README) works
without any extra setup.

## Development

```bash
# Build (debug)
cargo build

# Test (93+ unit + integration tests)
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

> **Tip:** the `mold` linker speeds up incremental Linux builds noticeably.
> It is **optional** — see comments in `.cargo/config.toml` for how to opt in.

## Contributing

We love contributions of every size — code, docs, bug reports, ideas.

1. Read [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow (humans and AI agents are both first-class contributors).
2. Pick an open issue, or open a new one to discuss large changes first.
3. Run `cargo test` and `cargo clippy -- -D warnings` before pushing.
4. Sign your commits and follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

Reporting a security issue? Please follow [SECURITY.md](SECURITY.md) instead of opening a public issue.

<!-- [media_suggestion:::prompt:```A friendly "Contributors welcome" badge-style illustration (800x400): stylised circular avatars (no real faces — abstract geometric shapes in cyan, magenta, lime) arranged in a half-circle around a Portail logo, with the text "Built by the community" below in clean sans-serif. Dark background. Inclusive, modern, no text in the avatars themselves.```] -->

## License

MIT — see [LICENSE](LICENSE).
