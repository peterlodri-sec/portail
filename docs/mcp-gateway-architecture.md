# MCP Gateway: Dual-Engine Architecture

Portail replaces the legacy Python MCP sidecar (`uv + LiteLLM`) with a **dual Rust engine** — two complementary systems that can run independently or together.

## Engines

| Engine | What it does | Default port | Source |
|--------|-------------|-------------|--------|
| **`mcp-gateway`** (embedded) | Native Rust MCP routing — Meta-MCP, tool discovery, capability YAML system, ~110 REST capabilities, circuit breakers, OWASP security controls | 39400 | [`mcp-gateway` crate](https://crates.io/crates/mcp-gateway) — MikkoParkkola/mcp-gateway |
| **zeroclaw** (sidecar) | Agent runtime — web dashboard, Telegram/Discord/Matrix/webhook channels, A2A agent discovery, config API, event stream, cron | 42617 | [`zeroclaw` binary](https://github.com/zeroclaw-labs/zeroclaw) — zeroclaw-labs/zeroclaw |

## Backend modes

Set via `[mcp].backend` in config:

```toml
[mcp]
# mcp_gateway   — embedded only (default)
# zeroclaw      — sidecar only
# dual          — both (redundancy + dashboard + channels)
# python        — legacy uv sidecar (deprecated)
backend = "mcp_gateway"

mcp_gateway_host = "127.0.0.1"
mcp_gateway_port = 39400
mcp_gateway_config = ""       # optional YAML file

zeroclaw_binary = "zeroclaw"
zeroclaw_host = "127.0.0.1"
zeroclaw_port = 42617
```

## Architecture

```
Portail proxy (:8787)
├── /mcp/* ──► mcp-gateway (:39400)    # MCP routing
│   │            ├── MCP backends (stdio/http/sse)
│   │            └── Capability YAML
│
├── /mcp/* ──► zeroclaw (:42617)       # zeroclaw-only: MCP + dashboard
│   │            ├── Web dashboard
│   │            ├── Channels (Telegram/Discord/Matrix/Webhook)
│   │            ├── A2A discovery
│   │            └── Config API / event stream
│
└── (legacy) /run/portail/mcp.sock     # Python (deprecated)
```

In `dual` mode `mcp-gateway` handles MCP tool routing while zeroclaw provides the dashboard, channels, and A2A. In `mcp_gateway` mode no sidecar runs at all — everything is in-process.

## Why this split

- **`mcp-gateway`** is specialised: Meta-MCP (13-16 tool surface instead of every backend), capability YAML system for REST APIs without writing MCP servers, circuit breakers, OWASP security controls, SHA-256 integrity pinning. It's the right tool for MCP routing.
- **zeroclaw** is an agent runtime: channels for human interaction (Telegram/Discord/etc), a web dashboard for chat/memory/config, and A2A for agent-to-agent discovery.
- They're independent libraries — you use what you need.

## Files

```
crates/portail-mcp-gateway/src/lib.rs   -- embedded mcp-gateway launcher
crates/portail-agents/src/zeroclaw.rs   -- zeroclaw sidecar agent
src/mcp/mod.rs                          -- backend dispatch
src/config.rs                           -- McpBackend enum, McpConfig
src/proxy.rs                            -- route_mcp() routing
```

## Install

ZeroClaw (for zeroclaw/dual):
```bash
curl -fsSL https://raw.githubusercontent.com/zeroclaw-labs/zeroclaw/master/install.sh | bash
```

The `mcp_gateway` backend needs no external binary — it's compiled in.
