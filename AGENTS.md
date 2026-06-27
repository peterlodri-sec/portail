# AGENTS.md — Portail Project Cross-Reference Hub

> For AI agents and humans exploring this codebase. Start here.

---

## Quick Links

| You want to... | Go here |
|----------------|---------|
| Understand what Portail is | [`README.md`](README.md) |
| Get started in 5 minutes | [`START_HERE.md`](START_HERE.md) |
| Architecture overview | [`docs/architecture/DESIGN.md`](docs/architecture/DESIGN.md) |
| Network design | [`docs/architecture/NETWORK_DESIGN.md`](docs/architecture/NETWORK_DESIGN.md) |
| Product strategy | [`docs/architecture/PRODUCT.md`](docs/architecture/PRODUCT.md) |
| Development roadmap | [`LOOP_STATE.md`](LOOP_STATE.md) |
| v2.0 plan | [`docs/architecture/V2_0_PLAN.md`](docs/architecture/V2_0_PLAN.md) |
| OSI/network layers | [`docs/layers/README.md`](docs/layers/README.md) |
| Contribute | [`docs/contributors/CONTRIBUTING.md`](docs/contributors/CONTRIBUTING.md) |
| OpenCode multiplexer integration | [`docs/contributors/OPENCODE_MUX.md`](docs/contributors/OPENCODE_MUX.md) |
| Release process | [`docs/contributors/RELEASE.md`](docs/contributors/RELEASE.md) |
| CI agent design | [`docs/contributors/CHORE_BOT_DESIGN.md`](docs/contributors/CHORE_BOT_DESIGN.md) |
| V4 plan (ACP + maki + Zed) | [`docs/architecture/V4_PLAN.md`](docs/architecture/V4_PLAN.md) |
| V3 roadmap | [`docs/architecture/V3_ROADMAP.md`](docs/architecture/V3_ROADMAP.md) |
| Abstraction review | [`docs/architecture/ABSTRACTION_REVIEW.md`](docs/architecture/ABSTRACTION_REVIEW.md) |
| Report security issue | [`SECURITY.md`](SECURITY.md) |
| Code of conduct | [`docs/contributors/CODE_OF_CONDUCT.md`](docs/contributors/CODE_OF_CONDUCT.md) |
| Version history | [`CHANGELOG.md`](CHANGELOG.md) |
| Cheatsheets | [`docs/cheatsheets/`](docs/cheatsheets/) |
| ADK-Rust reference | [`docs/adk-rust-cheatsheet.md`](docs/adk-rust-cheatsheet.md) |
| gitoxide (gix) cheatsheet | [`docs/cheatsheets/gix.md`](docs/cheatsheets/gix.md) |
| Axum server patterns | [`docs/cheatsheets/axum.md`](docs/cheatsheets/axum.md) |
| Tokio async patterns | [`docs/cheatsheets/tokio.md`](docs/cheatsheets/tokio.md) |
| Tower middleware | [`docs/cheatsheets/tower.md`](docs/cheatsheets/tower.md) |
| Rust AI stack (candle, burn, rig) | [`docs/cheatsheets/rust-ai-stack.md`](docs/cheatsheets/rust-ai-stack.md) |
| MCP servers reference | [`docs/cheatsheets/mcp.md`](docs/cheatsheets/mcp.md) |
| Connection upgrader pattern | [`docs/cheatsheets/connection-upgrader.md`](docs/cheatsheets/connection-upgrader.md) |
| Command intercept demo | [`docs/demos/git-intercept-demo.md`](docs/demos/git-intercept-demo.md) |
| Release-audit v2 plan | [`docs/RELEASE_AUDIT_V2_PLAN.md`](docs/RELEASE_AUDIT_V2_PLAN.md) |
| Dependencies + features | [`Cargo.toml`](Cargo.toml) |
| Route spec (60+ endpoints) | [`spec.routes.toml`](spec.routes.toml) |
| Dev commands | [`Taskfile.yml`](Taskfile.yml) |
| Build pipeline | [`.github/workflows/`](.github/workflows/) |
| Entry point | [`src/main.rs`](src/main.rs) |
| AppState + module list | [`src/lib.rs`](src/lib.rs) |
| HTTP router (all routes) | [`src/proxy.rs`](src/proxy.rs) |
| Nix flake | [`flake.nix`](flake.nix) |
| Ownership | [`CODEOWNERS`](CODEOWNERS) |
| NullClaw agent (heartbeat) | [`crates/portail-agents/src/nullclaw.rs`](crates/portail-agents/src/nullclaw.rs) |
| Drift-detect CI agent | [`crates/portail-agents/src/ci/drift.rs`](crates/portail-agents/src/ci/drift.rs) |
| Spec-verify CI agent | [`crates/portail-agents/src/ci/spec_verify.rs`](crates/portail-agents/src/ci/spec_verify.rs) |
| Fuzz-route CI agent | [`crates/portail-agents/src/ci/fuzz_route.rs`](crates/portail-agents/src/ci/fuzz_route.rs) |
| Chore-bot CI agent | [`crates/portail-agents/src/ci/chore.rs`](crates/portail-agents/src/ci/chore.rs) |
| Release-audit | [`src/release_audit.rs`](src/release_audit.rs) |

---

## File Organization

```
portail/
├── src/                        # Source code
│   ├── main.rs                 # Entry point, CLI dispatch, server startup
│   ├── lib.rs                  # AppState, module declarations
│   ├── proxy.rs                # HTTP router (60+ endpoints)
│   ├── config.rs               # TOML config (Config, load, defaults)
│   ├── types.rs                # BoundedMeta, hot-path types
│   ├── config_watcher.rs       # Self-healing config file watcher
│   ├── nats_bridge.rs          # NATS event bus bridge
│   ├── shutdown.rs             # Panic hooks, graceful shutdown
│   ├── store.rs                # Event store (SQLite + Turso backends)
│   ├── rate_limit.rs           # Token bucket rate limiter
│   ├── auth.rs                 # JWT + API-key authentication
│   ├── sessions.rs             # Per-session analytics
│   ├── supervisor.rs           # Background task supervisor
│   ├── file_cache.rs           # Content-addressable file cache
│   ├── graphql.rs              # Async-graphql schema + router
│   ├── telemetry.rs            # OTLP trace export
│   ├── drift.rs                # Drift detect (capture/replay)
│   ├── spec_verify.rs          # Spec verify (route vs golden)
│   ├── fuzz_route.rs           # Fuzz route (crash detector)
│   ├── lints.rs                # Custom lint rules
│   ├── a2a/                    # Agent-to-Agent protocol (card, task, WS)
│   ├── a2c/                    # Agent-to-Consumer chat API
│   ├── cdn/                    # CDN cache (cache.rs, manager)
│   ├── ci/                     # CI status webhook
│   ├── cli/                    # CLI: dashboard, complexity, init, learn, setup
│   ├── discovery/              # Service discovery
│   ├── dns/                    # DNS: DoH, isolation, reliability
│   ├── dpdk/                   # DPDK kernel bypass (stub)
│   ├── ebpf/                   # eBPF observability (stub)
│   ├── events/                 # Event log: ring buffer + SSE
│   ├── gateway/                # AI gateway forwarding
│   ├── godfather/              # System monitor + webhook alerts
│   ├── hooks/                  # Prompt injection: CRUD + matching
│   ├── hyper_engine/           # Hyper low-level HTTP (stub)
│   ├── iouring/                # io_uring async I/O (stub)
│   ├── mcp/                    # MCP sidecar proxy
│   ├── nullclaw/               # Network-native heartbeat agent
│   ├── plugins/                # tinyurl, tracer, redis_cache
│   ├── proxy/                  # Proxy module README
│   └── sentinel/               # Health watchdog
│
├── tests/                      # Integration + layer + proptest
│   ├── v0_2_integration.rs     # Integration tests (35)
│   ├── layer_tests.rs          # Layer tests (15)
│   └── proptests.rs            # Property-based tests (5)
│
├── docs/                       # Documentation
│   ├── architecture/           # Architecture + product docs
│   ├── layers/                 # OSI model, network layers
│   └── contributors/           # Contributing, release, code of conduct
│
├── .github/                    # GitHub CI/CD
│   ├── workflows/              # CI + release + agent workflows
│   ├── ISSUE_TEMPLATE/         # Bug + feature templates
│   └── PULL_REQUEST_TEMPLATE.md
│
├── scripts/                    # Shell scripts
│   └── rust-chore.sh           # Chore CI agent
│
├── nix/                        # Nix packaging
├── packaging/                  # deb, rpm, snap, flatpak
├── benches/                    # Criterion benchmarks
├── notebooks/                  # Marimo notebooks
│
├── README.md                   # Project front page
├── START_HERE.md               # One-page onboarding
├── PRODUCT.md                  # ... moved to docs/architecture/
├── CHANGELOG.md                # Version history
├── LOOP_STATE.md               # Development state + roadmap
├── SECURITY.md                 # Security policy
├── TASKS.md + TASKS_V0.2.md   # Task lists
├── CODEOWNERS                  # File ownership
├── Cargo.toml                  # Rust dependencies + features
├── flake.nix                   # Nix flake
├── Dockerfile                  # Docker build
├── Taskfile.yml                # Dev commands (task check, test, lint)
└── spec.routes.toml            # Golden route spec
```

---

## CI Agent Policy

| Agent | Blocks CI? | Exit Code | Status |
|-------|-----------|-----------|--------|
| complexity | ❌ never | always 0 | SHIPPED |
| drift-detect | ❌ never | always 0 | SHIPPED |
| spec-verify | ❌ never | always 0 | SHIPPED |
| fuzz-route | ⚠️ only on panic | 1 on crash, 0 otherwise | SHIPPED |
| chore-bot | ❌ never | always 0 | SHIPPED |
| clippy | ✅ always | 1 on warning | SHIPPED |
| test | ✅ always | 1 on failure | SHIPPED |

---

## Key Technical Details

- **Language**: Rust (edition 2024, MSRV 1.85)
- **Runtime**: tokio (multi-thread, work-stealing)
- **HTTP**: axum + tower + reqwest
- **Serialization**: serde + toml + serde_json
- **Cache**: Moka (in-memory) + cacache (disk, mmap)
- **Hashing**: blake3 (SIMD), AHash (maps)
- **Database**: SQLite (rusqlite, WAL mode), Turso (libsql, opt-in)
- **Events**: Ring buffer + broadcast channel + NATS bridge
- **Observability**: OTLP (gRPC), Prometheus, event log, TUI dashboard
- **Auth**: JWT (RS256/ES256), static API keys, bypass paths
- **Rate limiting**: Token bucket (governor), per-key, per-endpoint
- **Packaging**: Cargo, Nix, Docker, deb/rpm/snap/flatpak, Homebrew, AUR
- **CI**: GitHub Actions, self-hosted runners, cosign signing
- **Tests**: 174 (+ proptest), 0 compiler warnings
