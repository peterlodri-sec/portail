# Portail — Product Definition

**Version**: 1.4.0 | **Date**: 2026-06-26 | **Status**: Production

---

## Summary

Portail is a unified, self-hosted proxy and gateway that gives enterprises a single
control plane for AI infrastructure. One binary. Start in 5 minutes. Scale to fleet.

---

## Core Offering

> **One-liner**: *Your AI infrastructure's nervous system. Fast, smart, observable, and entirely yours to control.*

| Dimension | What Portail does |
|-----------|-------------------|
| **AI Gateway** | Stream proxy to LiteLLM / OpenAI / Anthropic / Ollama |
| **MCP Gateway** | Unix socket sidecar for MCP tool execution |
| **CDN Cache** | Two-tier cache (Moka memory + blake3 disk) with NATS invalidation |
| **Agent Protocol** | Google A2A: agent cards, task lifecycle, WebSocket streaming |
| **Agent Chat** | A2C: human-facing chat API with tool use |
| **Hook Injection** | Per-message / per-event prompt injection, CRUD API |
| **Event System** | Ring buffer + broadcast + NATS bridge for distributed observability |
| **TUI Dashboard** | Live network sparklines, cache ratios, config health, keyboard nav |
| **DNS** | DoH resolution, network isolation, reliability (cache + fallback) |
| **Observability** | OTLP traces, Prometheus metrics, `/dashboard` health snapshot |
| **Security** | Rate limiting (token bucket), JWT/API-key auth, security headers, HSTS |

---

## 5 Core Problems Solved

| Problem | How Portail Solves It |
|---------|----------------------|
| **Cost explosion** | 30-40% reduction via two-tier caching (memory + disk). Don't pay twice for the same prompt. |
| **Fragmentation** | One binary replaces LiteLLM proxy, Kong/Nginx, Redis cache, Prometheus, Jaeger, DNS resolver, and agent orchestrator. |
| **Security gaps** | Self-hosted, air-gappable. Rate limiting, JWT/API-key auth, network isolation, audit log via event store. Zero external dependencies at runtime (NATS/Redis opt-in). |
| **Integration complexity** | REST, A2A, GraphQL, WebSocket, MCP — all in one binary. Hooks inject system prompts without touching app code. |
| **Visibility blind spots** | E2E tracing (OTLP), live TUI dashboard, Prometheus metrics, event log with retention. Every request, every agent lifecycle event, every cache miss — observable. |

---

## Design System — 4 Core Pillars

### 1. Performance First

```
Language:     Rust (zero-cost abstractions, no GC)
I/O:          tokio async runtime, zero-copy where possible
Hashing:      SIMD-optimized blake3 (cache keys), AHash (maps)
Cache:        Moka (in-memory, TTL-aware) + cacache (disk, mmap)
Compression:  UPX for release binaries (<10MB compressed)
Lookup:       <5ms p99 cache hit, <1ms local moka lookup
```

### 2. Composable Architecture

Every feature is opt-in. No feature requires another. Start with just the proxy,
add cache when costs matter, add hooks when you need prompt injection, add agents
when you need orchestration.

```
portail serve                    # proxy only
portail serve --with-cache       # + caching
portail serve --with-agents      # + A2A/A2C
portail serve --with-dashboard   # + live metrics
```

Everything behind feature gates. No compile-time bloat for unused features.

### 3. Observable by Default

Every request produces: trace spans, metrics counters, event log entry.
No config required. Zero instrumentation code in your application.

```
Client → Portail → Upstream
         │
         ├─ Trace span (request_id, duration, status)
         ├─ Prometheus counter (http_requests_total)
         ├─ Event log entry (agent lifecycle)
         └─ Session recording (per-session analytics)
```

### 4. Developer Experience

- **Local-first**: `cargo install portail && portail serve` — 5 minutes to first request
- **Zero-config**: sensible defaults, everything works without `portail.toml`
- **Two interfaces**: CLI (`portail serve`, `portail status`, `portail events`) + interactive TUI (`portail`)
- **Init wizard**: `portail init` generates config interactively
- **Self-documenting**: `portail docs`, `portail learn <topic>`, architecture docs in repo

---

## Market Positioning

| Segment | Value Proposition |
|---------|-------------------|
| **Startups** | Free (MIT), low barrier. `cargo install portail`. No infra team needed. |
| **Mid-Market** | Self-hosted, auditable. Control your AI spend without vendor lock-in. |
| **Enterprise** | Air-gappable, RBAC, multi-tenant. SOC2-observable. NixOS module with hardening. |
| **Platform Teams** | Composable. Pick the layers you need. Mix-and-match backends (SQLite/Turso, Moka/Redis). |

**Philosophy**: *AI infrastructure should be like Kubernetes or PostgreSQL — a
self-hosted, battle-tested control plane that you own, not a SaaS you rent.*

---

## Competitive Differentiation

### Portail vs. LiteLLM Proxy

| Aspect | LiteLLM | Portail |
|--------|---------|---------|
| Language | Python | **Rust** (10-50x throughput, no GIL) |
| Caching | Redis only | **Two-tier** (Moka memory + blake3 disk) |
| Agent protocol | None | **A2A + A2C + MCP** native |
| Observability | Via LiteLLM callbacks | **Built-in**: OTLP, Prometheus, event log |
| DNS / Network isolation | None | **DoH + network isolation + DNS cache** |
| Deployment | Python process | **Single static binary** (<10MB UPX) |

### Portail vs. Kong / Nginx

| Aspect | Kong / Nginx | Portail |
|--------|-------------|---------|
| AI-aware routing | Plugin-based (Lua) | **Native**: AI Gateway routes, hook injection |
| Agent orchestration | None | **Built-in**: A2A task lifecycle, task store |
| Cache strategy | Proxy cache (URL-based) | **Prompt-aware**: semantic cache via blake3 |
| Live dashboard | Kong Manager (separate) | **Built-in TUI**: no separate service |
| Startup time | ~seconds (Lua VM) | **Sub-millisecond** (native binary) |

### Portail vs. Anthropic Workbench

| Aspect | Workbench | Portail |
|--------|-----------|---------|
| Scope | Anthropic-only | **Multi-provider**: OpenAI, Anthropic, LiteLLM, Ollama |
| Hosting | SaaS only | **Self-hosted**, air-gappable |
| Extensibility | API-only | **Full control**: modify proxy logic, add hooks |
| Observability | Dashboard | **E2E traces + Prometheus + event store** |

### Portail vs. Building Your Own

| Dimension | DIY | Portail |
|-----------|-----|---------|
| Time to production | 2-4 months | **5 minutes** |
| Maintenance burden | Full-time engineer | **0 ops** (background sentinel, health checks) |
| Security audit | You build it | **Pre-audited**: rate limiting, auth, HSTS, security headers |
| Feature completeness | Partial | **Full**: cache, observability, agents, DNS, TUI |
| Open source | Your code | **MIT**: inspect, fork, contribute |

---

## Value Propositions

### 1. Cost Reduction (30-40%)
Two-tier caching prevents duplicate API calls. Moka memory cache for hot keys
(<1ms lookup), blake3 disk cache for warm keys (~5ms). Cache hit ratio
visible in real-time via TUI dashboard.

### 2. Security & Central Control
Self-hosted, air-gappable. Rate limiting per API key, JWT authentication,
network isolation via DNS hooks, audit log with event store retention.
No data leaves your infrastructure.

### 3. Vendor Independence
Switch AI providers without changing your application. Portail normalizes
the `POST /v1/chat/completions` interface across OpenAI, Anthropic,
LiteLLM, and local Ollama. No vendor lock-in.

### 4. Observability
E2E request tracing from client → portail → upstream. Every span
instrumented. Prometheus metrics for dashboards. Event log for debugging.
`/dashboard` endpoint for health snapshots. Session analytics per
client ID.

### 5. Agent Orchestration
Google A2A protocol native. Create tasks, stream messages, track
lifecycle. GraphQL API for complex queries. WebSocket for real-time
agent communication. Agent fleet (godfather, nullclaw, sentinel) for
background monitoring.

### 6. Developer Experience
Local-first. One binary, no dependencies. `cargo install portail`.
Interactive TUI for exploration. `portail init` wizard for config.
`portail learn` for education. `portail docs` for reference.

---

## Cross-Reference: Key Project Files

| File | Purpose | Link |
|------|---------|------|
| `README.md` | Project overview, features, install | [/](README.md) |
| `START_HERE.md` | One-page everything-explainer | [/](START_HERE.md) |
| `DESIGN.md` | Architecture, module responsibilities, CLI | [/](DESIGN.md) |
| `NETWORK_DESIGN.md` | Network architecture, middleware, OTP model | [/](NETWORK_DESIGN.md) |
| `LOOP_STATE.md` | Version state, roadmap, CI agent policy | [/](LOOP_STATE.md) |
| `docs/V2_0_PLAN.md` | v2.0 4-week production plan | [/](docs/V2_0_PLAN.md) |
| `docs/CHORE_BOT_DESIGN.md` | Rust chore CI agent spec | [/](docs/CHORE_BOT_DESIGN.md) |
| `CHANGELOG.md` | Version history, features per release | [/](CHANGELOG.md) |
| `RELEASE.md` | Release process, signing, verification | [/](RELEASE.md) |
| `SECURITY.md` | Security policy, reporting | [/](SECURITY.md) |
| `CONTRIBUTING.md` | Contributing guidelines | [/](CONTRIBUTING.md) |
| `Cargo.toml` | Dependencies, features, profiles | [/](Cargo.toml) |
| `spec.routes.toml` | Golden route spec (60+ endpoints) | [/](spec.routes.toml) |
| `src/proxy.rs` | Main HTTP router (all routes) | [src/proxy.rs](src/proxy.rs) |
| `src/lib.rs` | AppState, module declarations | [src/lib.rs](src/lib.rs) |
| `src/main.rs` | Entry point, CLI dispatch, server startup | [src/main.rs](src/main.rs) |
| `src/gateway/README.md` | AI gateway forwarding architecture | [src/gateway/README.md](src/gateway/README.md) |
| `src/a2a/README.md` | A2A protocol implementation | [src/a2a/README.md](src/a2a/README.md) |
| `src/a2c/README.md` | A2C chat interface | [src/a2c/README.md](src/a2c/README.md) |
| `src/cdn/README.md` | CDN cache architecture | [src/cdn/README.md](src/cdn/README.md) |
| `src/events/README.md` | Event log + SSE streaming | [src/events/README.md](src/events/README.md) |
| `src/hooks/README.md` | Prompt injection system | [src/hooks/README.md](src/hooks/README.md) |
| `src/sentinel/README.md` | Background health watcher | [src/sentinel/README.md](src/sentinel/README.md) |
| `Taskfile.yml` | Dev commands: check, test, lint, release | [/](Taskfile.yml) |
| `scripts/rust-chore.sh` | Chore CI agent (fix/verify/report) | [scripts/](scripts/rust-chore.sh) |
| `.github/workflows/` | CI pipeline (build, test, release, agents) | [.github/workflows/](.github/workflows/) |
| `flake.nix` | Nix flake: packages, devShell, checks | [/](flake.nix) |
| `Dockerfile` | Multi-stage Docker build | [/](Dockerfile) |
| `portail.example.toml` | Example configuration | [/](portail.example.toml) |

---

## Strategic Roadmap

```
Phase 1: Foundation ✅ (v0.1 → v1.4, shipped 2026-06-26)
  Proxy, cache, hooks, events, A2A, A2C, MCP, DNS
  Rate limiting, auth, event store, OTLP
  5 CI agents (complexity, drift, spec, fuzz, chore)
  174 tests, 0 warnings

Phase 2: Production Stable 🚧 (v2.0, targeting 2026-07-01)
  StoreBackend trait — pluggable SQLite | Turso
  DNS reliability — cache, fallback, DNSSEC
  DPDK + io_uring production promotion
  Panic hooks, graceful shutdown, chaos testing
  Abstractions — Cache/DNS/IoEngine traits
  DX — portail doctor, zero-config, dead code removal

Phase 3: Enterprise (v2.1, targeting 2026-08-01)
  RBAC — role-based access control
  Multi-tenant — namespace isolation per tenant
  Turso distributed event store (multi-region)
  GraphQL subscriptions (real-time query push)
  Audit log compliance (SOC2, GDPR retention policies)

Phase 4: Edge (v2.2, targeting 2026-09-01)
  io_uring native (Linux 5.1+)
  DPDK kernel bypass (dedicated NIC req'd)
  eBPF observability (syscall tracing)
  P2P cache invalidation (Iroh / content-addressed)
  Remote support tunnel (RustDesk)

Phase 5: AI-Native (v3.0, targeting 2026-12-01)
  Function calling router (tool use via MCP)
  Prompt versioning (git-like prompt history)
  Cost attribution (per-model, per-user, per-request billing)
  Semantic cache (embedding-based dedup)
  Agent mesh (multi-node A2A federation)
```

---

## Quickstart

```bash
# Install
cargo install portail

# Generate config
portail init

# Start server
portail serve

# Check health
curl http://localhost:8787/healthz

# View dashboard
curl http://localhost:8787/dashboard | jq

# Stream events
curl http://localhost:8787/events

# Or use the CLI
portail status      # server status
portail events      # recent events
portail config show # current config
portail health      # health check
```
