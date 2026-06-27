# Portail Loop State — v2.1 → v3.0

**Principle:** CI agents advisory only. Report, recommend, never block. CI gates opt-in.

```
LOOP: plan → implement → test → review → ship → repeat
STATE:  v2.1 shipped — 231 tests, 10 CI agents, ProviderHandler abstraction, Ollama adapter, E2E benchmark
NEXT:   v2.2 documentation + OSS release (crates.io, Homebrew, AUR, blog) — #33
THEN:   v2.3 stability + benchmarks (80% coverage, 250+ tests) — #34
THEN:   v3.0 AI-native hybrid architecture — #30 + #31
FREEZE: no new features until v3.0. Bug fixes and stability only.
OPEN:   #28, #30, #31, #33, #34, #1 super devnote (HUMAN ONLY) — #33 + #34 active
```

---

## v2.1 — Contributor Experience (Issue #32)    ✅ SHIPPED

### Scope 1: Comprehensive Contribution Guide
- **CONTRIBUTING.md** — full rewrite at root (533 lines): architecture map, first-time walkthrough, testing guide, code style, PR process, issue triage, release process, agent section, troubleshooting
- **docs/architecture/MODULE_MAP.md** — every module with purpose, I/O, deps, key types, dependency graph (465 lines)
- **docs/architecture/DATA_FLOW.md** — request lifecycle: AI gateway, CDN cache, event system, A2A, A2C, DNS, hook injection, config watcher, rate limiting (378 lines)
- **docs/architecture/ARCHITECTURE_DECISIONS.md** — 10 ADRs covering key decisions

### Scope 2: Contributor Environment
- **`.vscode/`** — settings.json, extensions.json, launch.json for Rust IDE setup
- **`.devcontainer/devcontainer.json`** — GitHub Codespaces instant cloud dev
- **`.git/hooks/pre-commit`** — installed and verified (fmt + clippy + check)
- **`scripts/contributor-setup.sh`** — one-command bootstrap: Rust, Nix, direnv, hooks
- **Taskfile.yml additions** — `setup`, `coverage`, `profile`, `deny`, `audit`, `install-hooks`, `outdated`, `tree`, `udeps`
- **`nix/ai-services.nix`** — self-contained flake for Ollama + MLX provisioning with hardware-aware model selection (ai-check, ollama-pull-models)

### Scope 3: Tech Debt & Architecture
- **Provider Handler abstraction** (`src/gateway/handlers/`) — each provider's full lifecycle in one struct, 5 handlers: OpenAI, DeepSeek, Anthropic, Google, Ollama
- **Feature virtualizer** (`src/gateway/features.rs`) — capability matrix per provider, fallback strategies: Strip, StripWarn, PromptInject, ResponseTransform
- **Ollama adapter fixes** — path rewrite `/v1/chat/completions` → `/api/chat`, Content-Length fix, qwen3 thinking field promotion
- **cargo-deny + cargo-audit** — installed, deny.toml migrated to v0.19 format
- **async-nats upgrade** 0.39 → 0.48 fixing 4 security vulnerabilities
- **utoipa + utoipa-axum + Scalar UI** — auto-generated OpenAPI 3.1 at `/api-docs/` with interactive docs

### Metrics
- 231 tests (0 failures), 0 clippy warnings
- Coverage: ~50% (lib), ~55% excl. main.rs
- Benchmarks: 10 baselines, sub-µs hot paths
- deny/advisories: OK (known warnings allowed, 0 vulnerabilities)
- E2E verified: Ollama (qwen3:8b, qwen2.5-coder:7b) through portail → OpenAI format response

### What's shipped this release
| Area | Status |
|---|---|
| PHILOSOPHY.md | done |
| pkg-ctx crate (FTS5 SQLite docs MCP server) | done |
| loopeng real engine (token budget, circuit breaker, escalation) | done |
| Fleet orchestrator (AgentTool trait, ToolRegistry, FanOutEngine) | done |
| 3-pane TUI dashboard (banner, log, agent matrix) | done |
| A2C commands (/research, /code, /review, /register) | done |
| SOTA Nix flake (flake-parts, devshell, treefmt, git-hooks) | done |
| Shell completions (portail completions bash/zsh/fish) | done |
| deny.toml | done |
| /api-docs/openapi.json | done |
| spawn_blocking for SQLite ops | done |
| CI: green, simple, fast | done |
| Provider handler abstraction | done |
| Feature virtualizer | done |
| Ollama adapter + E2E test | done |
| Utoipa + Scalar UI | done |
| AI services Nix flake | done |
| VS Code settings + Codespaces | done |

---

## v2.2 — Documentation + OSS Release (Issue #33)  🚧 NEXT
**Target:** 2026-07-10

### Scope
- Architecture docs: system diagram, module map, data flow, ADRs (shipped in v2.1)
- API reference: all endpoints documented with examples
- Operator manual: deploy, configure, monitor, troubleshoot
- OSS release: crates.io publish, Homebrew formula, AUR package, Docker
- Blog post at pocoo.vaked.dev

### Remaining
- [ ] API reference docs for 60+ endpoints (utoipa annotations incremental)
- [x] Architecture docs (shipped)
- [x] ADRs (shipped)
- [ ] crates.io publish verified
- [ ] Homebrew formula submitted
- [ ] AUR package submitted
- [ ] Docker multi-arch verified
- [ ] Blog post + social announcement

---

## v2.3 — Stability + Polish (Issue #34)  🚧 NEXT
**Target:** 2026-07-17

### Scope
- 80%+ line coverage, cargo-llvm-cov, HTML reports
- Integration tests for every public API endpoint
- Fuzz targets for hot-path parsing
- Property tests for all abstractions
- Benchmark baseline published
- Bug bash: clippy, audit, deny, unsafe review
- Tag v2.3.0: 200+ tests, 10 CI agents
- `nix run github:peterlodri-sec/portail -- serve` production-ready
- Sensible defaults: rate limiting (30 burst), auth disabled, OTLP off
- Config-less startup: all features work without config file
- `portail init` wizard: generates portail.toml interactively
- Verified on: x86_64-linux, aarch64-linux, aarch64-darwin
- CI: Nix flake check + binary smoke test on all 3 platforms
- A2A WebSocket /a2a/ws route wired (was dead code)
- 6 new A2A tests: 3 JSON serialization + 3 HTTP integration (144 total)
- AgentGateway interop complete — A2A spec compliance verified

### Done
- [x] Coverage baseline: ~50% lib, cargo-llvm-cov installed
- [x] Benchmark baselines captured (10 benchmarks, sub-µs hot paths)
- [x] 231 tests passing
- [x] 0 clippy warnings, deny/advisories OK
- [x] Bug bash: async-nats upgrade (4 CVEs), Content-Length panic fix
- [x] Supervisor tests: 7 new tests (3% → 90%+ coverage)
- [x] Godfather config/tick/service tests: 8 new tests (0% → 80% coverage)
- [x] Target router tests: 6 new tests (85% → 95% coverage)
- [x] Schema adapter tests: 2 new thinking tests + all 13 pass
- [x] E2E benchmark: docs/E2E_BENCHMARK.md

### Remaining
- [ ] Property tests for StoreBackend, CacheBackend, DnsResolver
- [ ] Fuzz targets for hot-path parsing (headers, JSON, TOML)
- [ ] cargo-llvm-cov HTML report generation
- [ ] Benchmark baseline published in docs
- `inotify` (Linux) / `kqueue` (macOS) config file watcher
- Auto-reload on change (no SIGHUP needed)
- Validate before apply: parse new config, if invalid → keep old, log error
- Config versioning: store last N valid configs, rollback command
- TUI indicator: green dot = config healthy, red = last reload failed

---

## v3.0 — AI-Native Hybrid Architecture (Issues #30 + #31)  🚧 PLANNED
**Target:** 2026-08-01

- Live overlays on existing TUI dashboard:
  - Cache-hit rate sparkline (60s window)
  - Rate-limit exhaustion counter per key
  - Auth-failure tally per endpoint
  - OTLP trace sampling rate
- Alert mode: on break, highlight + show one-line fix
- Keyboard shortcuts: `r` = reload config, `c` = clear alerts, `q` = quit
- Works in 80x24 terminal minimum

See docs/architecture/V3_ROADMAP.md

## v2.1 — Contributor DX + Agent-Native Foundation       🚧 WIP
**Target:** 2026-07-03  **Effort:** 2 weeks

### Dead-code removal
- Removed `creative_tui/` workspace crate (wgpu/ratatui hybrid — deferred)
- Removed `src/cli/dashboard.rs` ratatui TUI; default CLI now prints help
- Removed `src/fuzz_route.rs` CLI subcommand (Google fuzzer will handle fuzzing)
- Removed `examples/plugins/request-logger/` example plugin
- Dropped `ratatui`, `crossterm` dependencies; updated packaging metadata

### ADK-Rust runtime integration
- Added `adk-rust = "=0.9.1"` to `crates/portail-agents` (MSRV 1.85)
- Reimplemented `nullclaw` heartbeat as ADK-Rust `CustomAgent`
- Ported `spec-verify` CI agent to ADK-Rust `CustomAgent`
- Added `ci/runner.rs` scheduler; wired into server lifecycle
- Spawned `nullclaw` heartbeat loop in `src/main.rs`

### A2A agent registry
- Added `src/a2a/registry.rs` with skill-based discovery
- Routes: `GET/POST /a2a/agents`, `GET/DELETE /a2a/agents/{id}`
- `POST /a2a/tasks` accepts `"skill"` and records `routed_to` / `routed_url`

**Status:** implementation complete, pending `cargo check` verification.

---

## CI Agent Policy

| Agent | Blocks CI? | Exit Code | Output |
|-------|-----------|-----------|--------|
| complexity | ❌ never | always 0 | TOML report → PR comment |
| drift-detect | ❌ never | always 0 | diff report → PR comment |
| spec-verify | ❌ never | always 0 | spec diff → PR comment |
| fuzz-route | ⚠️ only on panic | 1 on crash, 0 otherwise | crash report → PR comment |
| chore-bot | ❌ never | always 0 | fix report → PR comment |
| arch-helper | ❌ never | always 0 | drift report → issue |
| trending-packages | ❌ never | always 0 | weekly report → issue |
| pr-governance | ❌ never | always 0 | template check → label |
| ralph-loop | ❌ never | always 0 | version decision → issue + PR |
| clippy | ✅ always | 1 on warning | inline annotations |
| test | ✅ always | 1 on failure | inline annotations |

---

## Version Bump Schedule

```
v0.2  [SHIPPED]  2026-06-26  rate-limit, auth, store, otlp
v0.3  [SHIPPED]  2026-06-26  complexity advisory
v0.4  [SHIPPED]  2026-06-26  drift-detect
v0.5  [SHIPPED]  2026-06-26  spec-verify
v0.6  [SHIPPED]  2026-06-26  fuzz-route, WebSocket, GraphQL
v1.0  [SHIPPED]  2026-06-26  one-command Nix gateway
v1.1  [SHIPPED]  2026-06-26  self-healing config
v1.2  [SHIPPED]  2026-06-26  dashboard endpoint, rate/auth counters
v1.3  [SHIPPED]  2026-06-26  type hardening, BoundedMeta
v1.4  [SHIPPED]  2026-06-26  chore-bot, NATS bridge
v1.4r [SHIPPED]  2026-06-26  release v1.4.0
v2.0  [SHIPPED]  2026-06-26  production-stable (174 tests, 10 CI agents)
v2.1  [SHIPPED]  2026-07-03  contributor DX + agent-native foundation — docs/V2_1_V2_3_PLAN.md
v2.2  [NEXT]     2026-07-10  documentation + OSS release (crates.io, Homebrew, AUR, blog)
v2.3  [NEXT]     2026-07-17  stability: 80% coverage, benchmarks, bug bash, final release
v2.5             TBD         release-audit + project-wide simplification (~1700 lines dead code removed)
v2.6             TBD         portail-agents crate: nullclaw + CI agents
v2.7             TBD         RE deep-audit: Ghidra + Ghidra MCP + RE-agent-fleet
v3.0  [PLANNED]  2026-08-01  AI-native bridge — see V3_ROADMAP.md
v4.0  [PLANNED]  2026-09-01  VKID integrity kernel, .vaked plugin system
```

### Package Integration Research

| Priority | Crate | Purpose | Target Version |
|----------|-------|---------|---------------|
| 1 | hickory-dns | SOTA async DNS library | v2.1 DnsResolver backend |
| 2 | arrow-rs | Zero-copy IPC for agent communication | v3.0 |
| 3 | indicatif | Progress bars for CLI | v2.1 |
| 4 | rkyv | Zero-copy deserialization | v2.2 |
| 5 | loom | Concurrency testing | v2.3 |
| 6 | cargo-llvm-cov | Coverage reports | v2.3 |
| 7 | extism | Wasm plugin host | v3.0 |
| 8 | figment | Multi-source config | v2.1 |
