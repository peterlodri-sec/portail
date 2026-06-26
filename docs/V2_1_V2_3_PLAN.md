# Portail v2.1–v2.3 — DX-Focused Roadmap

## Goal

Make Portail the **best Rust project to contribute to**: flawless onboarding,
instant dev environment, comprehensive guides, and SOTA tooling.

---

## v2.1 — Contributor Experience (1 week)

### Scope 1: Comprehensive Contribution Guide

**`CONTRIBUTING.md` rewrite** — from current skeleton to full guide:

- [ ] First-time contributor walkthrough (step-by-step, takes <5 min)
- [ ] Architecture map: where to find each module, what it does
- [ ] Local dev setup: Nix flake, direnv, cargo commands
- [ ] Testing guide: what tests exist, how to run them, how to add new ones
- [ ] Code style: rustfmt config, clippy rules, naming conventions
- [ ] PR process: branch naming, commit messages, review expectations
- [ ] Issue triage: how to pick up issues, labels explained
- [ ] Release process: how versioning works, how to publish

### Scope 2: E2E Contributor Environment

**One-command contributor setup**:

```bash
# The only command a new contributor needs
nix develop
# or
direnv allow
# Provides: rustc, cargo, clippy, nextest, mold/zld, sccache, just, pre-commit hooks
```

- [ ] `scripts/contributor-setup.sh` — installs nix + direnv + git hooks
- [ ] `.github/CONTRIBUTING.md` — enhanced with troubleshooting
- [ ] `flake.nix` — complete devShell with all tools (✅ already done)
- [ ] `Taskfile.yml` — `task setup` command for contributors
- [ ] Pre-commit hook: runs `cargo fmt --check`, `cargo clippy`, `task test-fast`
- [ ] GitHub Codespaces / Gitpod config for instant cloud dev

### Scope 3: Package Research

**Categories to evaluate** (for integration or inspiration):

| Category | Top Crates | Status for Portail | Action |
|----------|-----------|-------------------|--------|
| **CLI** | clap 4 (✅), indicatif (progress bars), dialoguer (prompts) | clap in use | Evaluate dialoguer for `portail init` |
| **Config** | figment, config-rs | toml in use | Consider figment for multi-source config |
| **Observability** | tracing (✅), tracing-appender (✅), opentelemetry (✅), metrics (✅) | All in use | — |
| **HTTP** | axum (✅), reqwest (✅), hyper, hickory (DNS) | axum/reqwest in use | Evaluate hickory for DNS |
| **Serialization** | serde (✅), simd-json, rkyv (zero-copy) | serde in use | Evaluate rkyv for IPC |
| **Cache** | moka (✅), quick-cache, mini-moka | moka in use | — |
| **Database** | sqlx (✅), rusqlite (✅), sea-orm, diesel | sqlx primary | Evaluate sea-orm as ORM |
| **IPC** | arrow-rs, capnp, flatbuffers | none yet | Arrow for v3.0 agent communication |
| **Crypto** | aws-lc-rs (✅), ring, rustls, boring | aws-lc-rs feature | — |
| **Testing** | proptest (✅), nextest (✅), mutagen (mutation testing), loom (concurrency) | proptest in use | Evaluate loom for async testing |
| **Profiling** | samply, dhat, cargo-flamegraph | — | Add profiling to devshell |
| **Wasm** | extism, wazero, wasmtime | — | Evaluate for v3.0 agents |
| **Linkers** | mold (✅), wild (✅), zld (✅) | All configured | — |

### Scope 4: Local Dev Environment Optimization

- [ ] `task setup` — one-command full environment bootstrap
- [ ] `task profile` — run samply/dhat on current binary
- [ ] `task mutate` — run cargo-mutagen for mutation testing
- [ ] `task coverage` — run cargo-llvm-cov for coverage report
- [ ] `.vscode/settings.json` — recommended extensions + settings
- [ ] `.vscode/launch.json` — debug configuration for `portail serve`

---

## v2.2 — Documentation + OSS Release (1 week)

### Scope 1: Architecture Documentation

- [ ] `docs/architecture/README.md` — full system diagram (Mermaid) with component interactions
- [ ] `docs/architecture/MODULE_MAP.md` — every module with: purpose, inputs, outputs, deps
- [ ] `docs/architecture/DATA_FLOW.md` — request lifecycle: ingress → middleware → handler → upstream
- [ ] `docs/contributors/ARCHITECTURE_DECISIONS.md` — ADR-style decisions log

### Scope 2: API Reference

- [ ] Every public endpoint documented (REST, A2A, GraphQL, WebSocket)
- [ ] Request/response examples for each endpoint
- [ ] Auth requirements annotated per endpoint
- [ ] `portail docs` generates and serves locally (✅ already done)

### Scope 3: Operator Manual

- [ ] Deploy guide: Docker, Nix, systemd, Kubernetes
- [ ] Configure guide: every config option explained
- [ ] Monitor guide: Prometheus metrics, Grafana dashboard JSON
- [ ] Troubleshoot guide: common errors + solutions

### Scope 4: OSS Release

- [ ] `CHANGELOG.md` consolidated for v2.0.0 release
- [ ] crates.io: `cargo publish` verified
- [ ] Homebrew formula (`portail.rb`) submitted
- [ ] AUR package (`PKGBUILD`) submitted
- [ ] Docker: `ghcr.io/peterlodri-sec/portail:latest` verified
- [ ] Blog post: "Portail: Your AI Infrastructure's Nervous System" (pocoo.vaked.dev)
- [ ] Social: X/Twitter thread, Reddit r/rust, Hacker News Show HN

---

## v2.3 — Stability + Polish (1 week)

### Scope 1: Test Suite Hardening

- [ ] Coverage requirement: 80%+ line coverage (currently ~70%)
- [ ] `task coverage` — generates HTML report via cargo-llvm-cov
- [ ] Add integration tests for every public API endpoint
- [ ] Add fuzz targets for hot-path parsing (headers, JSON, TOML)
- [ ] Property tests for all abstractions (StoreBackend, CacheBackend, DnsResolver)

### Scope 2: Bug Bash

- [ ] Fix all clippy warnings (ensure `-D warnings` passes)
- [ ] Fix all `cargo audit` issues (dependency CVEs)
- [ ] Fix all `cargo deny` issues (license + duplicate deps)
- [ ] Manual review of all `unsafe` blocks (✅ zero, enforced by `#![forbid(unsafe_code)]`)

### Scope 3: Performance Baseline

- [ ] Benchmark baseline published in `docs/architecture/BENCHMARKS.md`
- [ ] Throughput: requests/sec for each endpoint
- [ ] Resource: memory usage at idle and under load
- [ ] Comparison: portail vs nginx vs liteLLM proxy

### Scope 4: Final Release

- [ ] Tag `v2.3.0`
- [ ] All CI green: 10 agents, 200+ tests, 80%+ coverage
- [ ] Release binaries: Linux x86_64 + aarch64, macOS aarch64
- [ ] Blog post + social announcement

---

## Version Schedule

```
v2.0.0 [SHIPPED]  2026-06-26  production-stable (174 tests, 10 CI agents)
v2.1   [PLANNED]  2026-07-03  contributor DX: guide, env, package research
v2.2   [PLANNED]  2026-07-10  documentation + OSS release
v2.3   [PLANNED]  2026-07-17  stability + polish + benchmarks
v3.0   [PLANNED]  2026-08-01  AI-native: Go+Rust+Wasm hybrid
```

---

## Package Integration Candidates (Priority-Ordered)

1. **hickory-dns** — replace custom DNS client with SOTA async DNS library (DNS trait backend)
2. **arrow-rs** — zero-copy IPC for agent communication (v3.0 foundation)
3. **indicatif** — progress bars for `portail init`, `portail setup`
4. **rkyv** — zero-copy deserialization for hot-path JSON
5. **loom** — concurrency testing for async code
6. **cargo-llvm-cov** — coverage reports (dev dependency)
7. **extism** — Wasm plugin host for v3.0 agent sandbox
8. **figment** — multi-source config (env + file + defaults)
