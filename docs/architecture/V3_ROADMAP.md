# Portail v3.0 Roadmap — AI-Native Bridge

**Target:** 2026-08-01
**Theme:** Bridge from static proxy to AI-native agent runtime.
**Prerequisite for:** V4 VKID Integrity Kernel

---

## Milestones

### P0 — Connection Upgrader (3 days)
Upgrade HTTP connections to WebSocket, raw TCP, or PTY on the fly
without dropping the socket. Critical for A2A/WS, MCP sidecar handoff.

```
[HTTP request] ───► [101 Switching Protocols] ───► [WebSocket frame loop]
                         │
                    raw_fd handoff to dedicated worker
```

**Files:**
- `src/upgrader.rs` — ConnectionUpgrader struct with `upgrade_to_ws()`, `upgrade_to_raw()`
- `src/a2a/mod.rs` — wire upgrader into the existing WS handler
- `src/mcp/mod.rs` — use upgrader for sidecar socket handoff

**Deliverable:** `portail serve` upgrades A2A WebSocket connections using the
upgrader module. 3+ tests covering success, fallback, and raw fd extraction.

---

### P1 — WASM MCP Sidecar (5 days)
Replace the Python MCP sidecar (slow, fragile, requires uv + litellm)
with a WASM-based sidecar using Extism or Wasmtime.

```
[axum HTTP] ──► [Rust proxy] ──► [wasmtime runtime]
                                      │
                                 ┌─────┴─────┐
                                 │ MCP server │
                                 │ (compiled  │
                                 │  to .wasm) │
                                 └───────────┘
```

**Files:**
- `crates/portail-mcp-wasm/` — WASM MCP server host
- `src/mcp/mod.rs` — replace `Command::new("uv")` with wasmtime instantiation

**Deliverable:** No Python dependency. MCP servers run as WASM modules.
~50KB binary instead of ~200MB Python venv.

---

### P2 — BOW (Backend Object Warehouse) (4 days)
Agent-accessible encrypted secret store. Replaces `.env` for production.

```
portail bow set ANTHROPIC_API_KEY sk-ant-xxx  → encrypted at rest
portail bow list                               → shows names, not values
portail bow get ANTHROPIC_API_KEY --into-env   → exports to env
```

**Files:**
- `src/bow.rs` — BOW engine: encrypted SQLite store + keyring unlock
- `src/cli/bow.rs` — CLI subcommands
- `src/config.rs` — `[bow]` config section

**Deliverable:** `portail bow` CLI works. Secrets survive reboot. Auto-unlock
via TPM/enclave or passphrase.

---

### P3 — Capability Graph (Basic) (4 days)
DAG-based config language. Define capabilities, lower to Config.

```toml
[capability.deploy]
uses = ["target:anthropic-fast", "mcp:filesystem", "mcp:github"]
```

Lowerer walks the DAG, checks all deps satisfied, emits Config struct.

**Files:**
- `src/capability.rs` — `CapabilityGraph`, `Capability` enum, `lower()`, `verify()`

**Deliverable:** `portail config verify` checks capability DAG.
`portail config lower --capability deploy` emits resolved config.

---

### P4 — Rust AI Stack (3 days)
Local inference via mistral.rs + candle. Connect to target router.

```
[target:local-mistral] ──► mistral.rs HTTP server ──► candle (Metal/CUDA)
```

**Files:**
- `src/gateway/local.rs` — local inference gateway
- `Cargo.toml` — add `mistralrs` or `candle` behind feature flag

**Deliverable:** `portail serve` with `[[targets]] provider = "local"` serves
models via mistral.rs. `curl http://localhost:8787/v1/chat/completions -d
'{"model":"llama-3.2-3b-instruct","messages":...}'` works.

---

## Schedule

| Week | Milestone | Tests |
|------|-----------|-------|
| Jul 28 | P0 Connection Upgrader | 177+ |
| Jul 31 | P1 WASM MCP Sidecar | 185+ |
| Aug 04 | P2 BOW Secret Store | 195+ |
| Aug 07 | P3 Capability Graph | 200+ |
| Aug 11 | P4 Rust AI Stack | 210+ |
| Aug 14 | v3.0 Release | 210+ |

## Dependencies

| Crate | Purpose | Milestone |
|-------|---------|-----------|
| `extism` / `wasmtime` | WASM runtime | P1 |
| `aes-gcm` / `orion` | Secret encryption | P2 |
| `petgraph` | DAG for capability graph | P3 |
| `mistralrs` / `candle` | Local inference | P4 |
