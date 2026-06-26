# Portail v2.0 — First Production-Stable Release

## Goal

Ship the first production-stable release of Portail.
Every feature hardened, every edge case tested, every component
observable, every security surface audited.

## Timeline: 4 weeks (2026-07-01 → 2026-08-01)

```
Week 1: Stability    — chaos testing, edge cases, crash recovery
Week 2: Production   — TLS, DNS, health, deploy guide
Week 3: Scale        — load testing, benchmarks, profiling
Week 4: Polish       — docs, security audit, release
```

---

## Week 1 — Stability Engineering

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
- **Event store recovery** — SQLite WAL mode, crash-consistent

### 1.3 Fuzz Expansion
- Extend `fuzz-route` CI agent: 10k probes → 100k probes
- Add fuzz targets for: config parser, TOML deserialization, JSON payloads up to 1MB
- Property-based testing with `proptest` for `BoundedMeta`, `RateLimiter`, `AuthState`

### Tests: 200+ target (from 156)

---

## Week 2 — Production Hardening

### 2.1 TLS Everywhere
- **Let's Encrypt integration** — ACME client, auto-renew (via `portail setup`)
- **mTLS** — client certificate verification for internal services
- **TLS config hardening** — TLS 1.3 only, cipher suite audit, HSTS preload
- **Certificate pinning** — optional pin for known upstreams

### 2.2 DNS Reliability
- **DNS cache** — TTL-aware in-memory cache with negative caching
- **Fallback resolvers** — Cloudflare (1.1.1.1) → Google (8.8.8.8) → Quad9 (9.9.9.9)
- **Split-horizon DNS** — internal vs external resolution
- **DNSSEC validation** — verify RRSIGs on resolved records

### 2.3 Health Check Matrix
- `/healthz` — basic alive (exists)
- `/readyz` — all dependencies connected (exists)
- `/livez` — ready + rate limiter not exhausted + config healthy (NEW)
- **Dependency health** — NATS, Redis, SQLite, upstream AI endpoint

### 2.4 Deploy Guide
- **systemd unit** — `portail.service` with restart=always, memory limits, security directives
- **Docker Compose** — portail + Redis + NATS + Jaeger full stack
- **Kubernetes** — Helm chart with configmap, secrets, HPA, PDB
- **NixOS module** — hardening flags, firewall rules, systemd integration

---

## Week 3 — Scale & Performance

### 3.1 Load Testing
- **wrk2** / **oha** benchmarks: 1k, 10k, 100k req/s
- **Upstream latency** — measure portail overhead vs direct-to-upstream
- **Cache hit ratio** — benchmark Moka + Redis at scale

### 3.2 Memory Profiling
- **dhat** / **heaptrack** — find allocation hot spots
- **BoundedMeta** — verify 16-entry limit prevents memory leaks
- **EventLog** — verify 2000-entry ring stays bounded under load
- **SessionStore** — add TTL eviction (1h default)

### 3.3 Connection Pooling
- **reqwest pool** — configure max idle, timeout, keep-alive
- **Redis pool** — connection manager tuning
- **NATS pool** — subscribe concurrency limits

### 3.4 Benchmark Suite
- `benches/hot_paths.rs` — existing criterion benchmarks
- Add: config parse bench, rate limit check bench, auth verify bench
- Add: JSON roundtrip bench (AgentCard, Task, AgentEvent)
- Add: BoundedMeta insert/lookup bench vs FxHashMap

---

## Week 4 — Polish & Release

### 4.1 Security Audit
- **cargo audit** — check all deps for CVEs
- **cargo deny** — license compliance, duplicate deps, advisory DB
- **Manual audit** — review all `unsafe` blocks (minimal → zero)
- **Auth bypass check** — verify all protected routes require auth
- **Rate limit bypass** — verify no path escapes rate limiter

### 4.2 Documentation
- **Architecture Guide** — full system diagram, component interactions
- **Operator Manual** — deploy, configure, monitor, troubleshoot
- **API Reference** — all REST + A2A + GraphQL + WebSocket endpoints
- **Integration Guide** — Claude Code, OpenCode, LiteLLM, custom clients

### 4.3 Observability
- **Grafana dashboard** — JSON export for import
- **Prometheus alerting rules** — high error rate, low cache hit rate, config unhealthy
- **OTLP best practices** — sampling strategy, span naming conventions

### 4.4 Release Checklist
- Tag `v2.0.0`
- All CI green (200+ tests, 7 advisory agents posting comments)
- crates.io publish
- Homebrew formula (portail.rb)
- AUR package (PKGBUILD)
- Docker multi-arch (linux/amd64 + linux/arm64)
- Nix `nix run github:peterlodri-sec/portail/v2.0.0 -- serve`
- Release blog post + changelog

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

## Decisions Needed

1. **Turso (libSQL)** — replace SQLite with distributed event store? Gain:5, Difficulty:2. Defer to v2.1 if not critical for stability.
2. **RustDesk tunnel** — remote support? Gain:5, Difficulty:4. Nice-to-have, defer to v2.1.
3. **Iroh P2P** — distributed cache inval? Gain:4, Difficulty:3. Defer to v2.1.
4. **DPDK / io_uring** — production-ready or experimental? Keep as experimental plugins, not required for v2.0 stability.
