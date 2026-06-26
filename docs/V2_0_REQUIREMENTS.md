# v2.0 Requirements Checklist

> From: [docs/architecture/V2_0_PLAN.md](docs/architecture/V2_0_PLAN.md)  
> Target: 4 weeks, 230+ tests, first production-stable release

## Week 1 — Stability + Turso ✅ (mostly done)

- [x] StoreBackend trait — pluggable SQLite/NATS-replicated backends
- [x] NATS-replicated backend (feature-gated: `store-nats`) — free, open source
- [x] Turso (libSQL) evaluated and replaced — not free, NATS replication instead- [x] proptest suite — 5 property tests (RateLimiter, BoundedMeta, AuthState)
- [x] Panic hooks + graceful shutdown module
- [x] EventLog.all_since() for NATS consumption
- [x] 164 → 174 tests
- [ ] Chaos testing suite — random kill, network partition, disk full, memory pressure
- [ ] Crash recovery — WAL verification, startup idempotency
- [ ] Fuzz expansion — 184 probes → more varied payloads

## Week 2 — Production + Engines (partially done)

- [x] DPDK + io_uring Cargo feature gates
- [x] DNS reliability — DnsCache (TTL-aware, negative caching)
- [x] DNS fallback — Cloudflare → Google → OpenDNS chain
- [ ] TLS: Let's Encrypt automatic (needs acme-lib crate, feature-gated)
- [ ] TLS: mTLS client certificate verification
- [ ] TLS: TLS 1.3-only enforcement, cipher suite audit
- [ ] Deploy guide — systemd unit, Docker Compose full stack, k8s Helm chart
- [ ] DPDK production integration tests (loopback mode, no NIC needed)
- [ ] io_uring production integration tests + benchmark vs epoll

## Week 3 — Scale + Abstractions

- [ ] Load testing — wrk2/oha benchmarks: 1k, 10k, 100k req/s
- [ ] Memory profiling — dhat/heaptrack, find allocation hot spots
- [ ] SessionStore TTL eviction (1h default)
- [ ] Cache trait — `CacheBackend` (Moka | Redis | Iroh)
- [ ] DNS trait — `DnsResolver` (DoH | system | custom)
- [ ] IoEngine trait — `IoEngine` (epoll | io_uring | DPDK)
- [ ] Benchmark suite — config parse, rate limit check, auth verify, JSON roundtrip
- [ ] Connection pooling — reqwest, Redis, NATS tuning

## Week 4 — DX + Security + Release

- [ ] `portail doctor` — checks system compatibility, prints fix suggestions
- [ ] Dead code removal — a2a/a2c dead routers, unused functions, orphaned helpers
- [ ] Merge small stubs — ebpf, dpdk, iouring, hyper_engine (collapse to 1 file each)
- [ ] `cargo audit` — check all deps for CVEs
- [ ] `cargo deny` — license compliance, duplicate deps
- [ ] OWASP top 10 review — injection, auth, exposure, XXE, misconfig
- [ ] Grafana dashboard JSON — import-and-go
- [ ] Prometheus alerting rules — high error rate, low cache hit, config unhealthy
- [ ] Architecture Guide — full system diagram, component interactions
- [ ] Operator Manual — deploy, configure, monitor, troubleshoot
- [ ] API Reference — all REST + A2A + GraphQL + WebSocket endpoints
- [ ] Tag v2.0.0 — all CI green, 230+ tests, crates.io publish
- [ ] Homebrew formula — portail.rb
- [ ] AUR package — PKGBUILD
- [ ] Release blog post

## Deferred to v2.1

- [ ] RustDesk remote support tunnel
- [ ] Iroh P2P cache invalidation
- [ ] Handy gesture recognition
- [ ] agent-browser MCP tool

---

## Dependency Additions Needed

| Crate | Purpose | Status |
|-------|---------|--------|
| `proptest` 1 | Property-based testing | ✅ Added (dev) |
| `acme-lib` or `rustls-acme` | Let's Encrypt | ❌ Not yet |

> **Removed**: `libsql` (Turso is paid). Replaced with NATS-replicated SQLite (free, uses existing `async-nats` dep).

---

## Test Targets

| Milestone | Target | Current |
|-----------|--------|---------|
| Week 1 done | 200+ | 174 |
| Week 2 done | 210+ | — |
| Week 3 done | 220+ | — |
| Week 4 (release) | 230+ | — |
