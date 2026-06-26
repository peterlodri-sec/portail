# Portail Network Design Document

**Version:** v0.6  **Date:** 2026-06-26

---

## Architecture Overview

```
                    ┌──────────────────────────────────────┐
                    │            portail binary            │
                    │                                      │
  Client ──TCP──►   │  axum router (HTTP/1.1, HTTP/2)     │
                    │    ├─ middleware stack               │
                    │    │   ├─ CORS                       │
                    │    │   ├─ Rate limit (governor)      │
                    │    │   ├─ Auth (JWT/API key)         │
                    │    │   ├─ Session recording          │
                    │    │   ├─ TraceLayer                 │
                    │    │   ├─ Body limit (10MB)          │
                    │    │   ├─ Metrics (prometheus)       │
                    │    │   ├─ Request ID                 │
                    │    │   ├─ Security headers           │
                    │    │   └─ Session middleware         │
                    │    │                                  │
                    │    ├─ Routes (60+ endpoints)         │
                    │    │   ├─ /v1/* → AI Gateway         │
                    │    │   ├─ /mcp/* → MCP proxy         │
                    │    │   ├─ /cdn/* → CDN cache         │
                    │    │   ├─ /a2a/* → Agent protocol    │
                    │    │   ├─ /graphql → GraphQL API     │
                    │    │   └─ /godfather/*, /sessions/*  │
                    │    │                                  │
                    │    └─ Fallback → AI Gateway          │
                    │                                      │
                    │  Background Processes (tokio tasks)  │
                    │    ├─ Godfather (sysinfo, webhook)   │
                    │    ├─ Sentinel (health, CDN scrub)   │
                    │    ├─ NullClaw (agent topology)      │
                    │    ├─ SIGHUP handler (config reload) │
                    │    ├─ NATS subscriber (CDN inval)    │
                    │    └─ EventStore retention purge     │
                    │                                      │
                    │  Storage                             │
                    │    ├─ moka (memory cache)            │
                    │    ├─ blake3 hashed disk cache       │
                    │    ├─ SQLite event store             │
                    │    └─ FxHashMap in-memory stores     │
                    └──────────────────────────────────────┘
```

---

## Actor / Supervision Model (Elixir OTP pattern in Rust)

You're thinking of the **Actor Model** (Erlang/Elixir OTP) and **CSP** (Go fibers/goroutines). In Rust, we have three approaches:

### What we already use (implicit supervision)

| Portail Module | Role | OTP Equivalent |
|---------------|------|---------------|
| `godfather` | System resource watchdog, webhook alerts | `Supervisor` + `:alarm_handler` |
| `sentinel` | Health checks, CDN scrub, auto-recovery | `Supervisor` with `restart: permanent` |
| `nullclaw` | Agent heartbeat + topology | `GenServer` with pub/sub |
| `mcp` | Unix socket sidecar launcher | `:simple_one_for_one` supervisor |
| `SIGHUP handler` | Live config reload | `:config_change` callback |

### Battle-tested Rust actor libraries

| Library | Stars | Approach | Use for |
|---------|-------|----------|---------|
| **`actix`** | 8.5k | Full actor framework, async, supervision | Heavyweight — too much for us |
| **`ractor`** | 1.2k | Lightweight actors, OTP-like supervision | Good fit for agent task queues |
| **`bastion`** | 2.8k | Fault-tolerant runtime, supervision trees | Good fit for background workers |
| **`tokio::task`** | — | Already using. `JoinSet` + `AbortHandle` = lightweight supervision | Ideal — we already have it |

### Recommendation: Tokio-native supervision (no new crate)

We already have the building blocks. Formalize with:

```
AppState
  ├─ Supervisor::spawn("godfather", run_godfather)  → restart: always
  ├─ Supervisor::spawn("sentinel",  run_sentinel)    → restart: always
  ├─ Supervisor::spawn("nullclaw",  run_nullclaw)    → restart: transient
  └─ Supervisor::spawn("nats",      purge_loop)      → restart: permanent
```

Where `Supervisor` is a thin wrapper (~50 LOC) around `tokio::spawn` + `JoinHandle` that:
- Tracks task health via `tokio::sync::watch`
- Auto-restarts on panic (with backoff)
- Exposes `/supervisor/status` endpoint
- Publishes `supervisor.task_crashed` events to EventLog

This gives us Elixir-style `let-it-crash` semantics with zero new dependencies.

---

## File Cache: Dedicated 500MB Always-On "Fort" Cache

### Current caching
| Layer | Backend | Purpose |
|-------|---------|---------|
| CDN cache | moka (memory) + blake3 (disk) | HTTP response caching |
| Redis cache | Redis + FxHashMap fallback | App-level key-value |
| No file cache | — | Intermediate build files, temp data |

### Proposal: `file_cache` module with `cacache` crate

**`cacache`** — Content-addressable cache (used by npm, cargo-binstall):
- Fixed-size LRU on disk (configurable: 500MB default)
- SHA-256 keyed, automatically evicts oldest on capacity exceeded
- Zero-config: `cacache::write("./cache", key, data)` / `cacache::read("./cache", key)`
- Already battle-tested in the JS ecosystem (npm uses it for global cache)

**Implementation:**
```toml
# Cargo.toml
cacache = { version = "14", features = ["mmap"] }
```

**`/file-cache` API:**
- `GET /file-cache/{key}` — retrieve cached file
- `PUT /file-cache/{key}` — store file
- `DELETE /file-cache/{key}` — evict
- `GET /file-cache/stats` — capacity, usage, hit rate

**Config:**
```toml
[file_cache]
enabled = true
path = "/var/cache/portail/files"
max_size = "500MB"
```

---

## Network Library Audit — Battle-Tested Rust Crates

### HTTP Server (current: axum 0.8)

| Crate | Stars | Notes |
|-------|-------|-------|
| **axum** | 19k | Using. Mature, tokio-native, middleware composition |
| `hyper` | 15k | Underlying engine. We use it implicitly. Direct use for zero-copy hot paths |
| `actix-web` | 22k | Larger ecosystem, different runtime (actix-rt). Not compatible with tokio |
| `warp` | 9.5k | Filter-based composition. Less active than axum |
| `poem` | 3.7k | Chinese ecosystem, OpenAPI-first. Niche |

**Verdict:** Stay with axum. It's the most actively maintained tokio-native framework.

### HTTP/3 + QUIC (future consideration)

| Crate | Stars | Notes |
|-------|-------|-------|
| **`quinn`** | 4k | Battle-tested QUIC implementation. Used by Cloudflare, Mozilla |
| **`h3`** | 1k | HTTP/3 on top of quinn. Early but functional |
| **`h3-quinn`** | 500 | Bridges h3 + quinn. Used in production by some |
| **`s2n-quic`** | 1.2k | AWS's QUIC implementation. Highly optimized |

**Verdict:** Defer HTTP/3 to v2.0. Axum's HTTP/2 is sufficient for all current use cases.

### WebSocket (current: axum built-in)

| Crate | Stars | Notes |
|-------|-------|-------|
| **axum `ws` feature** | — | Already using. Native, simple |
| `tokio-tungstenite` | 2k | More configurable. If we need custom WS handling |
| `async-tungstenite` | 700 | Same as above, different API |

**Verdict:** Axum's built-in WS is fine for now.

### gRPC (not yet used)

| Crate | Stars | Notes |
|-------|-------|-------|
| **`tonic`** | 10k | Battle-tested. Used by OTLP exporter already |
| `grpc-rust` | — | Less maintained |

**Verdict:** Already using tonic via opentelemetry-otlp. Can expose gRPC API in v2.0.

### Caching

| Crate | Stars | Notes |
|-------|-------|-------|
| **`moka`** | 2k | Using. Sync + async, TTL, TI, weight-based eviction |
| **`cacache`** | 500 | Content-addressable disk cache. npm's cache engine. |
| `quick_cache` | 700 | Faster than moka for simple cases. Lock-free. |
| `stretto` | 150 | Dgraph's Rust port. High throughput. |

**Verdict:** Add `cacache` for file cache. Keep `moka` for CDN cache.

### Concurrency Primitives

| Crate | Stars | Notes |
|-------|-------|-------|
| **`tokio`** | 28k | Already using. The standard Rust async runtime |
| `flume` | 1.5k | Faster mpsc channels than std/tokio. Lock-free |
| `crossbeam` | 7.5k | Advanced concurrent data structures |
| `dashmap` | 4k | Concurrent HashMap. We use `FxHashMap + RwLock` instead |

**Verdict:** `flume` could replace `tokio::sync::mpsc` in hot paths for 2-3x throughput.

---

## Current Features Inventory

### v0.1 (base)
- AI Gateway (LiteLLM/OpenAI/Anthropic proxying)
- MCP Gateway (Unix socket sidecar)
- CDN Cache (moka + blake3 disk)
- A2A Protocol (agent cards, task lifecycle)
- A2C Interface (chat API)
- Prompt Hooks (inject/CRUD)
- Event Log (ring buffer + SSE)
- TUI Dashboard (ratatui)
- Sentinel (health watchdog)
- NullClaw (agent topology)

### v0.2 (production hardening)
- Rate limiting (governor, token bucket, 429 + Retry-After)
- Auth middleware (API key + JWT RS256/ES256/HS256)
- Persistent event store (SQLite, retention, JSON export)
- OpenTelemetry OTLP export (Jaeger/Tempo)
- 12 ghost routers wired (ci, discovery, dns, tinyurl, tracer, redis_cache, godfather, nullclaw, ebpf, dpdk, iouring, hyper_engine)
- godfather + nullclaw background runners spawned

### v0.3 (CI advisory agents)
- Complexity bot (advisory, daily-once, never fails CI)
- Drift detect (production traffic replay, SHA-256 diff)
- Spec verify (53-route golden spec, auto-generated)
- Fuzz route (224 probes, 28 routes, crash detector)

### v0.4-0.6 (runtime features)
- Godfather: sysinfo monitoring (disk/memory/CPU) + webhook alerts
- Sessions: per-session analytics (tokens, cache, latency, hooks)
- WebSocket A2A: bidirectional agent streaming
- GraphQL API: query events/hooks/tasks, subscription live_events
- Type hardening: PartialEq on all configs, non_exhaustive, DriftStatus enum

---

## Raw Module Mapping (what uses what)

| Module | Key Libraries | Notes |
|--------|--------------|-------|
| `proxy` | axum, tower-http, metrics, governor | Router + middleware |
| `gateway` | reqwest | Upstream HTTP forwarding |
| `cdn` | moka, blake3, async-nats | Two-tier cache |
| `events` | tokio::sync::broadcast | Ring buffer + pub/sub |
| `hooks` | serde_json | Prompt injection |
| `a2a` | axum (ws), serde | Agent protocol |
| `a2c` | serde | Chat interface |
| `mcp` | tokio::net::UnixStream | Sidecar proxy |
| `graphql` | async-graphql, async-stream | Query/mutation/subscription |
| `store` | rusqlite | SQLite events |
| `auth` | jsonwebtoken | JWT + API keys |
| `rate_limit` | governor | Token bucket |
| `telemetry` | opentelemetry-otlp, tonic | OTLP gRPC |
| `godfather` | sysinfo | Resource monitoring |
| `sessions` | rustc-hash | Analytics store |
| `drift` | reqwest, sha2, hex | Traffic replay |
| `fuzz_route` | reqwest | Fuzzing |
| `spec_verify` | toml | Route spec enforcement |
| `complexity` | regex, walkdir | Big-O analysis |
| `tui` | ratatui, crossterm | Dashboard |
| `plugins` (tracer, tinyurl, redis_cache) | redis, rustc-hash | Utility plugins |

---

## Recommendations Summary

1. **Supervisor pattern**: Build a thin `Supervisor` (~50 LOC) wrapping `tokio::spawn`. No new crate needed.
2. **File cache**: Add `cacache` crate for 500MB always-on content-addressable file cache.
3. **Network**: Stay with axum for HTTP/1.1-2. Defer HTTP/3 + QUIC to v2.0.
4. **Channels**: Consider `flume` for hot-path mpsc channels (2-3x faster).
5. **gRPC**: Already have tonic via OTLP. Can expose native gRPC API in v2.0.
