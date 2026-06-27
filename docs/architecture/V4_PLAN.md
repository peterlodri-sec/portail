# Portail V4 — SOTA Gateway Architecture

**Target:** v4.0.0
**Status:** Active Development
**Theme:** Ship the foundation. No VKID, no capability graphs, no genesis seals.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    portail serve                              │
│                                                              │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐ │
│  │ Gateway  │   │ Router   │   │ Sandbox  │   │  Store   │ │
│  │ (axum)   │──▶│ (targets)│──▶│ (WASM)   │   │ (SQLite) │ │
│  └──────────┘   └──────────┘   └──────────┘   └──────────┘ │
│       │              │              │              │         │
│       ▼              ▼              ▼              ▼         │
│  HTTP/WS/A2A   Model routing   Extism plugins   BOW secrets│
│  MCP server    Local+cloud     Capability I/O   Event log  │
└─────────────────────────────────────────────────────────────┘
                          │
                    ┌─────┴─────┐
                    │  nushell  │  ← CLI/ops layer
                    │  + plugins│  ← query, formats, socket
                    │  + par-each│ ← parallel fleet ops
                    └───────────┘
                          │
                    ┌─────┴─────┐
                    │    nix    │  ← reproducibility layer
                    │  flake    │  ← pinned everything
                    │  deploy   │  ← NixOS modules
                    └───────────┘
```

---

## Milestones

### P0 — WASM MCP Sidecar (5 days)

Replace the Python MCP sidecar (slow, fragile, requires uv + litellm)
with a WASM-based sidecar using Extism.

```
[axum HTTP] ──► [Rust proxy] ──► [extism runtime]
                                      │
                                 ┌─────┴─────┐
                                 │ MCP server │
                                 │ (compiled  │
                                 │  to .wasm) │
                                 └───────────┘
```

**Why Extism over raw Wasmtime:**
- Plugin lifecycle management built-in
- Host functions for I/O without WASI complexity
- Polyglot: authors can write MCP servers in Rust, Go, TS, Python compiled to WASM
- Hot-reload without restarting portail

**Files:**
- `crates/portail-mcp-wasm/` — WASM MCP server host
- `src/mcp/mod.rs` — replace `Command::new("uv")` with extism instantiation

**Deliverable:** No Python dependency. MCP servers run as WASM modules.
~50KB binary instead of ~200MB Python venv.

---

### P1 — BOW (Backend Object Warehouse) (3 days)

Agent-accessible encrypted secret store. Replaces `.env` for production.

```
portail bow set ANTHROPIC_API_KEY sk-ant-xxx  → encrypted at rest
portail bow list                               → shows names, not values
portail bow get ANTHROPIC_API_KEY --into-env   → exports to env
```

**Design:**
- AES-256-GCM encryption at rest (ring or aes-gcm crate)
- Unlock via passphrase or TPM/enclave (when available)
- SQLite backend with WAL mode
- Audit log: every access recorded with timestamp + caller

**Files:**
- `src/bow.rs` — BOW engine: encrypted SQLite store + keyring unlock
- `src/cli/bow.rs` — CLI subcommands
- `src/config.rs` — `[bow]` config section

**Deliverable:** `portail bow` CLI works. Secrets survive reboot.

---

### P2 — A2A Protocol Support (3 days)

Standardized agent-to-agent interop via Google's A2A protocol.

```
Agent A ──(A2A JSON-RPC)──▶ Portail ──(A2A JSON-RPC)──▶ Agent B
                              │
                         Routes by agent card,
                         manages task lifecycle,
                         handles streaming
```

**Why:**
- A2A is the emerging standard (Google, 2025+)
- Replaces custom WebSocket handler with interop-ready protocol
- Enables multi-agent orchestration without custom code

**Files:**
- `src/a2a/` — A2A protocol handler (cards, tasks, streaming)
- Extend existing WS route to speak A2A JSON-RPC

**Deliverable:** Portail serves A2A agent cards. Tasks flow between agents.

---

### P3 — Local Inference Routing (3 days)

Route small-model requests to local inference via mistral.rs + candle.

```
[target:local-mistral] ──► mistral.rs HTTP server ──► candle (Metal/CUDA)
```

**Why:**
- Small models (3B-7B) are good enough for routing decisions, classification, embedding
- Saves API costs for high-volume, low-complexity requests
- Runs on the same box, no network latency

**Files:**
- `src/gateway/local.rs` — local inference gateway
- `Cargo.toml` — add `mistralrs` or `candle` behind feature flag

**Deliverable:** `portail serve` with `[[targets]] provider = "local"` serves
models via mistral.rs. `curl http://localhost:8787/v1/chat/completions` works.

---

### P4 — Nushell Fleet Ops (2 days)

Structured fleet management via nushell + par-each + plugins.

**Core commands (in `nushell/`):**

```nu
# Fleet health probe — parallel, typed, ordered
def "portail probe" [] {
    let targets = (open portail.toml | get targets | each { |t| {name: $t.name, url: $t.base_url} })
    $targets | par-each -k { |t|
        let health = (http get $"($t.url)/health" --timeout 3sec | from json)
        {name: $t.name, status: $health.status, version: $health.version}
    }
}

# Fleet deploy — parallel, ordered, CI exit code
def "portail deploy" [...hosts: string] {
    let results = ($hosts | par-each -k { |h|
        let res = (do { ^ssh $h "cd /opt/portail && cargo build --release" } | complete)
        {host: $h, ok: ($res.exit_code == 0)}
    })
    $results | each { |r| print $"(if $r.ok {'✓'} else {'✗'}) ($r.host)" }
    if ($results | where not ok | length) > 0 { exit 1 }
}

# Config drift detection — parallel across fleet
def "portail drift" [] {
    let nodes = (open portail.toml | get nodes)
    $nodes | par-each -k { |n|
        let remote_hash = (ssh $n.name "sha256sum /opt/portail/portail.toml" | split row " " | first)
        let local_hash = (open portail.toml | hash sha256)
        {node: $n.name, drift: ($remote_hash != $local_hash)}
    }
}
```

**Plugins installed:**
- `nu_plugin_query` — HTTP/JSON API probing
- `nu_plugin_formats` — format parsing (toml, yaml, etc.)

**Deliverable:** `portail probe`, `portail deploy`, `portail drift` work from any dev shell.

---

### P5 — OTLP → Grafana Observability (2 days)

Real observability via OpenTelemetry pipeline → Grafana.

```
[portail] ──OTLP/gRPC──▶ [otel-collector] ──▶ [Grafana] ──▶ dashboards
                                    │
                              [Tempo/Loki]
                              traces + logs
```

**What we already have:**
- OTLP trace export (src/telemetry.rs)
- Event store (src/store.rs)
- Prometheus metrics endpoint

**What we add:**
- OTLP log export (structured traces → Tempo)
- Dashboard provisioning (Grafana JSON dashboards)
- Alert rules (Prometheus → Alertmanager)

**Deliverable:** `docker compose up` gives full observability stack.
Grafana dashboards show request latency, error rates, agent activity.

---

## Dropped from Previous v4 Plan

| Component | Why Dropped |
|-----------|------------|
| VKID (seccomp-BPF kernel) | Too complex. We're not building a kernel. NixOS gives us most of this. |
| Capability Graph DAG | Figment already does config lowering. Don't build a second system. |
| Genesis Seal DNS notarization | Cool but not MVP. Ship first, attest later. |
| Maelstrom chaos agent | Nice-to-have. Do it in v5 after the foundation is solid. |

## Schedule

| Phase | Milestone | Target Date | Tests |
|-------|-----------|-------------|-------|
| P0 | WASM MCP Sidecar | Jul 07 | 185+ |
| P1 | BOW Secret Store | Jul 10 | 195+ |
| P2 | A2A Protocol Support | Jul 13 | 205+ |
| P3 | Local Inference Routing | Jul 16 | 210+ |
| P4 | Nushell Fleet Ops | Jul 18 | 210+ |
| P5 | OTLP → Grafana | Jul 20 | 215+ |
| **Release** | **v4.0.0** | **Jul 21** | **215+** |

## Dependencies

| Crate | Purpose | Phase |
|-------|---------|-------|
| `extism` | WASM runtime for MCP sidecar | P0 |
| `aes-gcm` | Secret encryption at rest | P1 |
| `ring` | Cryptographic operations | P1 |
| `mistralrs` or `candle` | Local inference | P3 |
| `opentelemetry` | OTLP export | P5 |
