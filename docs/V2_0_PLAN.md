# Portail v2.0 — First Production-Stable Release

## Goal

Ship the first production-stable release of Portail.
Every feature hardened, every edge case tested, every component
observable, every security surface audited. Turso distributed
event store. DPDK + io_uring promoted to production. DX
simplification pass: fewer knobs, better defaults, less config.

## Timeline: 4 weeks (2026-07-01 → 2026-08-01)

```
Week 1: Stability    — chaos testing, crash recovery, Turso migration
Week 2: Production   — TLS, DNS, DPDK/io_uring promo, deploy guide
Week 3: Scale        — load testing, benchmarks, abstraction pass
Week 4: DX + Release — simplification, docs, security audit, release
```

---

## Week 1 — Stability + Turso

### 1.1 Chaos Testing Suite
- **Random kill** — SIGKILL portail mid-request, verify no data loss
- **Network partition** — drop NATS/Redis/upstream, verify graceful degradation
- **Disk full** — fill /tmp, /var/cache, verify no crash (only log warnings)
- **Memory pressure** — 10k concurrent WebSocket connections, verify bounded memory
- **Slow upstream** — 30s latency from AI provider, verify timeout + retry
- **Config corruption** — malformed TOML, verify config watcher rejects + keeps old

### 1.2 Crash Recovery
- **Panic hooks** — install `std::panic::set_hook` that logs + flushes
- **Graceful shutdown** — `SIGTERM` drains connections (30s deadline), closes stores
- **Startup idempotency** — cold start, warm start, restart within 1s all work
- **Event store recovery** — WAL mode, crash-consistent

### 1.3 Fuzz Expansion
- Extend `fuzz-route` CI agent: 10k → 100k probes
- Property-based testing with `proptest`: `BoundedMeta`, `RateLimiter`, `AuthState`
- JSON payload fuzzing up to 1MB

### 1.4 Turso (libSQL) Distributed Event Store
- **Drop-in replacement** — same SQL interface as rusqlite, `StoreConfig` adds `turso_url`
- **Replicated SQLite** — events survive node loss, multi-region reads
- **Migration path** — export/import from SQLite → Turso, backward compatible
- **Config**:
  ```toml
  [store]
  enabled = true
  provider = "turso"  # "sqlite" | "turso"
  turso_url = "libsql://portail-org.turso.io"
  turso_auth_token = "$TURSO_TOKEN"
  ```
- **Dep**: add `libsql-client` or `libsql` crate (optional feature, `store-turso`)
- **CI**: integration test against Turso dev instance

### Target: 200+ tests (from 156)

---

## Week 2 — Production + DPDK/io_uring Promo

### 2.1 TLS Everywhere
- **Let's Encrypt** — ACME client, auto-renew (`portail setup`)
- **mTLS** — client certificate verification for internal services
- **TLS 1.3 only** — cipher suite audit, HSTS preload
- **Certificate pinning** — optional pin for known upstreams

### 2.2 DNS Reliability
- **DNS cache** — TTL-aware in-memory cache with negative caching
- **Fallback resolvers** — Cloudflare → Google → Quad9
- **Split-horizon DNS** — internal vs external resolution
- **DNSSEC validation** — verify RRSIGs

### 2.3 DPDK Production Promotion
- **Feature gate** — `dpdk` Cargo feature, off by default
- **Config validation** — detect unsupported platforms, graceful fallback
- **Integration tests** — DPDK smoke test (loopback mode, no NIC required)
- **Docs** — when to use, hardware requirements, performance tradeoffs
- **Metrics** — packets/sec, dropped/sec, DMA buffer usage

### 2.4 io_uring Production Promotion
- **Feature gate** — `iouring` Cargo feature, on by default on Linux 5.1+
- **Config validation** — detect kernel support, fallback to epoll
- **Integration tests** — io_uring smoke test (submit/completion path)
- **Benchmarks** — vs epoll: throughput, latency p99, syscall count
- **Metrics** — operations submitted/sec, completed/sec, avg latency

### 2.5 Deploy Guide
- **systemd unit** — restart=always, MemoryMax, ProtectSystem, NoNewPrivileges
- **Docker Compose** — portail + Redis + NATS + Jaeger + Turso full stack
- **Kubernetes Helm chart** — configmap, secrets, HPA, PDB, service monitor
- **NixOS module** — hardening flags, firewall rules

---

## Week 3 — Scale + Abstractions

### 3.1 Load Testing
- **wrk2/oha** — 1k, 10k, 100k req/s benchmarks
- **Upstream overhead** — portail latency vs direct-to-upstream
- **Cache ratio** — Moka + Redis hit rate at scale

### 3.2 Memory Profiling
- **dhat/heaptrack** — find allocation hot spots
- **BoundedMeta** — verify 16-entry limit under load
- **EventLog** — verify 2000-entry ring stays bounded
- **SessionStore** — add TTL eviction (1h default, configurable)

### 3.3 Connection Pooling
- **reqwest** — max idle, timeout, keep-alive tuning
- **Redis** — connection manager pool sizing
- **NATS** — subscribe concurrency limits, reconnect backoff

### 3.4 Abstraction Pass
- **Store trait** — `EventStore` (SQLite | Turso) behind a trait, no `if provider ==`
- **Cache trait** — `CacheBackend` (Moka | Redis | Iroh) behind a trait
- **DNS trait** — `DnsResolver` (DoH | system | custom) behind a trait
- **Engine trait** — `IoEngine` (epoll | io_uring | DPDK) behind a trait
- **Goal**: add a new backend without touching `main.rs` or `proxy.rs`

### 3.5 Benchmark Suite
- `benches/hot_paths.rs` — existing criterion
- Add: config parse bench, rate limit check, auth verify
- Add: JSON roundtrip (AgentCard, Task, AgentEvent)
- Add: BoundedMeta vs FxHashMap insert/lookup
- Add: io_uring vs epoll throughput/latency

### Target: 230+ tests

---

## Week 4 — DX + Security + Release

### 4.1 Simplification Pass (DX focus)
- **Kill `portail.toml` required fields** — everything has a sensible default
- **`portail serve` zero-config** — runs with no config file at all
- **`portail doctor`** — checks system compatibility (kernel, ports, deps), prints fix suggestions
- **Error messages** — replace "thread panicked at" with actionable `anyhow` errors
- **CLI discoverability** — `portail help` shows common workflows, not just flag soup
- **Remove dead code** — DPDK/io_uring stubs, unused `a2a::router()`, orphaned test helpers
- **Merge small modules** — `ebpf/mod.rs` (294 lines) → collapse stubs, keep only live code

### 4.2 Security Audit
- **cargo audit** — check all deps for CVEs
- **cargo deny** — license compliance, duplicate deps, advisory DB
- **Manual audit** — review all `unsafe`, auth bypass, rate limit bypass
- **OWASP top 10** — injection, auth, exposure, XXE, misconfig, etc.

### 4.3 Observability
- **Grafana dashboard JSON** — import-and-go
- **Prometheus alerting rules** — high error rate, low cache hit rate, config unhealthy
- **OTLP sampling strategy** — head-based, tail-based, span naming

### 4.4 Documentation
- **Architecture Guide** — full system diagram, component interactions
- **Operator Manual** — deploy, configure, monitor, troubleshoot
- **API Reference** — all REST + A2A + GraphQL + WebSocket endpoints
- **Integration Guide** — Claude Code, OpenCode, LiteLLM, custom clients

### 4.5 Release Checklist
- Tag `v2.0.0`
- 230+ tests, 7 CI agents green
- crates.io publish
- Homebrew formula + AUR package
- Docker multi-arch (linux/amd64 + linux/arm64)
- `nix run github:peterlodri-sec/portail/v2.0.0 -- serve`
- Release blog post + CHANGELOG

---

## v2.0 CI Agent Fleet (7 agents)

| Agent | Blocks CI? | Status |
|-------|-----------|--------|
| complexity | never | SHIPPED |
| drift-detect | never | SHIPPED |
| spec-verify | never | SHIPPED |
| fuzz-route | only on panic | SHIPPED |
| chore-bot | never | SHIPPED |
| clippy | always | SHIPPED |
| test | always | SHIPPED |

---

## Dependency Additions

| Crate | Purpose | Feature gate | Complexity |
|-------|---------|-------------|------------|
| `libsql` | Turso distributed SQLite | `store-turso` | Low (same SQL API) |
| `proptest` | Property-based testing | dev-only | Low |
| `acme-lib` or `rustls-acme` | Let's Encrypt | `tls-acme` | Medium |

---

## Deferred to v2.1

- RustDesk remote support tunnel
- Iroh P2P cache invalidation
- Handy gesture recognition
- agent-browser MCP tool
