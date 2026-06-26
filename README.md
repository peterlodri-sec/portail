# Portail

[![CI](https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml/badge.svg)](https://github.com/peterlodri-sec/portail/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/portail)](https://crates.io/crates/portail)
[![Docs.rs](https://img.shields.io/docsrs/portail)](https://docs.rs/portail)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

**Portail** — *French for "gateway"* — is a unified proxy that bundles three
infrastructure services behind a single port:

```
                           ┌─────────────┐
  ──▶  AI  API calls ───▶  │             │──▶ LiteLLM / OpenAI
  ──▶  MCP  tool calls ──▶  │   Portail   │──▶ Python MCP sidecar
  ──▶  CDN  asset fetches ▶  │   :8787    │──▶ S3 / MinIO origin
                           └─────────────┘
```

- **AI Gateway** — route and stream LLM requests to an upstream provider
- **MCP Gateway** — proxy Model Context Protocol tool calls to a Python sidecar
- **CDN Cache** — two-tier (memory + filesystem) HTTP cache with NATS invalidation

## Quick start

```bash
cargo install portail

# Default: AI Gateway on :8787, upstream at http://127.0.0.1:4000
portail
```

With a config file:

```bash
portail --config /etc/portail/config.toml
```

Or entirely via env vars:

```bash
PORTAIL_LISTEN=0.0.0.0:8787 \
PORTAIL_ENABLE_AI_GATEWAY=true \
PORTAIL_ENABLE_MCP=true \
PORTAIL_AI_UPSTREAM=http://127.0.0.1:4000 \
  portail
```

## Features

| Subsystem | What it does | Transport |
|-----------|-------------|-----------|
| **AI Gateway** | Proxies `/v1/*` to LiteLLM/compatible upstream, strips hop-by-hop headers, forwards `X-Forwarded-For` | HTTP/1.1, HTTP/2 |
| **MCP Gateway** | Proxies tool listing + invocation to a Python sidecar over Unix socket | Unix socket (framed binary) |
| **CDN Cache** | Caches HTTP responses from an origin — moka in memory, blake3-sharded files on disk, NATS invalidation | HTTP/1.1 |
| **Health** | `GET /healthz` and `GET /readyz` for load balancer probes | HTTP |
| **Metrics** | Prometheus endpoint at `GET /metrics` (when enabled) | HTTP |

## Configuration

Portail reads from three sources, in order of precedence (highest wins):

1. **CLI flags** — `--listen`, `--config`, `--cache-dir`, etc.
2. **Environment variables** — `PORTAIL_LISTEN`, `PORTAIL_CACHE_DIR`, etc.
3. **TOML file** — `portail --config /etc/portail/config.toml`

### Minimal

```toml
listen = "0.0.0.0:8787"

[ai_gateway]
enabled = true
upstream = "http://127.0.0.1:4000"
```

### Full reference

See [`portail.example.toml`](portail.example.toml) for a complete example with
all three subsystems configured.

| Variable | Flag | TOML key | Default | Description |
|----------|------|----------|---------|-------------|
| `PORTAIL_LISTEN` | `--listen` | `listen` | `0.0.0.0:8787` | Listen address |
| `PORTAIL_CACHE_DIR` | `--cache-dir` | `cache_dir` | `/var/cache/portail` | CDN cache on-disk path |
| `PORTAIL_CACHE_SIZE` | `--cache-size` | `cache_size` | `10g` | Max cache size |
| `PORTAIL_MCP_SOCKET` | `--mcp-socket` | `mcp_socket` | `/run/portail/mcp.sock` | MCP sidecar socket |
| `PORTAIL_ENABLE_AI_GATEWAY` | — | `ai_gateway.enabled` | `true` | Enable AI proxy |
| `PORTAIL_ENABLE_MCP` | — | `mcp.enabled` | `true` | Enable MCP proxy |
| `PORTAIL_ENABLE_CDN` | — | `cdn.enabled` | `false` | Enable CDN cache |
| `PORTAIL_AI_UPSTREAM` | — | `ai_gateway.upstream` | `http://127.0.0.1:4000` | AI upstream URL |
| `PORTAIL_CDN_ORIGIN` | — | `cdn.origin` | `http://127.0.0.1:9000` | CDN origin URL |
| `PORTAIL_NATS_URL` | — | — | — | NATS server for cache invalidation |

## Subsystems

### AI Gateway

Proxies all requests under `/v1/*` to the configured upstream. Strips
hop-by-hop headers (`Transfer-Encoding`, `Connection`, `Keep-Alive`, etc.),
injects `X-Forwarded-For`, and streams responses back chunk by chunk.

```
POST /v1/chat/completions   ──▶   http://127.0.0.1:4000/v1/chat/completions
GET  /v1/models             ──▶   http://127.0.0.1:4000/v1/models
```

### MCP Gateway

Proxies tool listing and tool call requests to a Python sidecar process over
a Unix socket using a length-prefixed framed protocol:

```
┌──────────┬──────────┬───────────┬─────────┬──────────────┐
│ method   │ path     │ headers   │ body    │              │
│ len:u16  │ len:u32  │ len:u32   │ len:u64 │              │
│ [bytes]  │ [bytes]  │ [JSON]    │ [bytes] │              │
└──────────┴──────────┴───────────┴─────────┴──────────────┘
```

The sidecar (`portail-mcp`) wraps LiteLLM's `MCPServerManager` and handles
transport diversity (SSE, Streamable HTTP, stdio).

### CDN Cache

Two-tier HTTP cache:

1. **Memory** — moka concurrent cache (fast, TTL-based eviction)
2. **Disk** — blake3-hashed files under `cache_dir/first2/last2/remainder`

On cache HIT, returns `X-Cache-Status: HIT`. On MISS, fetches from origin,
stores, and streams back. Supports prefix-based invalidation via NATS
(consumer on `index.invalidated.>`).

## NixOS module

Portail ships a NixOS module via its flake:

```nix
{
  inputs.portail.url = "github:peterlodri-sec/portail";

  outputs = { portail, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      modules = [
        portail.nixosModules.default
        {
          services.portail = {
            enable = true;
            enableAiGateway = true;
            enableMcp = true;
            enableCdn = false;
            openFirewall = true;
          };
        }
      ];
    };
  };
}
```

Fully hardened systemd services with `NoNewPrivileges`, `ProtectSystem`,
`PrivateTmp`, and separate user/group.

### Standalone Nix

```bash
nix run github:peterlodri-sec/portail
```

Or build the MCP sidecar:

```bash
nix build github:peterlodri-sec/portail#mcpPlugin
```

## Development

```bash
git clone https://github.com/peterlodri-sec/portail
cd portail
cargo build
cargo test
cargo clippy -- -D warnings
```

Requires Rust 1.85+ (edition 2024).

### Python sidecar

```bash
cd plugins/portail-mcp
uv sync
uv run portail-mcp --socket /tmp/portail-mcp.sock
```

## Roadmap

- [ ] **Rate limiting** — per-key and per-IP rate limiting for AI Gateway
- [ ] **Auth classification** — middleware that classifies API keys into tiers
- [ ] **Headroom compression** — lossless HTTP body compression for cache storage
- [ ] **OpenTelemetry** — distributed tracing via OTLP export
- [ ] **Config hot-reload** — SIGHUP reload without restart
- [ ] **Docker image** — multi-arch publishes to ghcr.io

## License

MIT — see [LICENSE](LICENSE).
