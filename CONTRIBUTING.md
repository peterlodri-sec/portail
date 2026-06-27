# Contributing to Portail

> **v2.1** — Welcome! Whether you're a human or an AI agent, this guide gets you from zero to contributing in under 5 minutes.

---

## Quick Start (First-Time Contributor)

```bash
# 1. Fork & clone
git clone https://github.com/<your-username>/portail
cd portail

# 2. One-command environment setup
nix develop              # auto: rustc, cargo, clippy, nextest, sccache, just, tools
# or
direnv allow             # if you have direnv + nix-direnv
# or keep it simple:
cargo check              # works with any Rust 1.85+ toolchain

# 3. Pick an issue
#    - Labels: "good first issue" | "help wanted" | "enhancement"
#    - Comment to claim it

# 4. Branch, code, test
git checkout -b feature/my-feature
# ... make your changes ...
cargo test               # 177+ tests, should all pass
cargo clippy -- -D warnings  # zero warnings enforced

# 5. Commit & PR
git commit -m "feat: my feature description"
git push origin feature/my-feature
# Open PR at https://github.com/peterlodri-sec/portail
```

**Total time: <5 minutes.** See [Architecture Map](#architecture-map) to find your way around.

---

## Table of Contents

- [Quick Start (First-Time Contributor)](#quick-start-first-time-contributor)
- [Architecture Map](#architecture-map)
- [Local Dev Environment](#local-dev-environment)
- [Testing Guide](#testing-guide)
- [Code Style & Conventions](#code-style--conventions)
- [PR Process](#pr-process)
- [Issue Triage](#issue-triage)
- [Release Process](#release-process)
- [For AI Agents](#for-ai-agents)
- [CI/CD Pipeline](#cicd-pipeline)
- [Troubleshooting](#troubleshooting)

---

## Architecture Map

Portail is a **single Rust binary** (`src/main.rs`) with a modular crate structure.

```
portail/
├── src/                          # Main binary modules (40+ modules)
│   ├── main.rs                   # Entry point, CLI dispatch, server startup
│   ├── lib.rs                    # AppState, module declarations, re-exports
│   ├── proxy.rs                  # axum HTTP router — 60+ endpoints
│   ├── config.rs                 # TOML config loader, validation
│   ├── types.rs                  # Hot-path types (BoundedMeta, etc.)
│   ├── auth.rs                   # JWT + API-key authentication
│   ├── rate_limit.rs             # Token bucket rate limiter (governor)
│   ├── sessions.rs               # Per-session analytics
│   ├── telemetry.rs              # OTLP trace export + JSON logging
│   ├── shutdown.rs              
│   ├── supervisor.rs             # Background task supervisor
│   ├── file_cache.rs             # Content-addressable file cache (blake3)
│   ├── graphql.rs                # async-graphql schema + router
│   ├── nats_bridge.rs            # NATS event bus bridge
│   ├── config_watcher.rs         # Self-healing config file watcher
│   ├── store.rs                  # Event store (SQLite + Turso)
│   ├── drift.rs                  # Drift detect (capture/replay)
│   ├── spec_verify.rs            # Spec verification vs golden
│   ├── fuzz_route.rs             # Route fuzzer (crash detection)
│   ├── lints.rs                  # Custom lint rules
│   ├── target_router.rs          # Multi-backend routing
│   ├── upgrader.rs               # Connection upgrader
│   ├── plugin_hooks.rs           # Plugin lifecycle hooks
│   ├── release_audit.rs          # Release audit trail
│   ├── test_utils.rs             # Shared test helpers
│   │
│   ├── a2a/                      # Agent-to-Agent protocol (Google)
│   ├── a2c/                      # Agent-to-Consumer chat API
│   ├── cdn/                      # CDN cache (moka + blake3 disk)
│   ├── ci/                       # CI status webhook
│   ├── cli/                      # TUI dashboard + CLI commands
│   ├── discovery/                # Service discovery
│   ├── dns/                      # DNS (DoH, isolation, fallback)
│   ├── events/                   # Ring buffer + SSE
│   ├── gateway/                  # AI upstream forwarding
│   ├── godfather/                # System monitor + alerts
│   ├── hooks/                    # Prompt injection CRUD
│   ├── mcp/                      # MCP sidecar proxy
│   ├── orchestrator/             # Loop engine orchestration
│   ├── plugins/                  # Built-in plugins
│   ├── sentinel/                 # Health watchdog
│   │
├── crates/                       # Workspace crates
│   ├── loopeng/                  # Loop engine: plan → execute → evaluate → decide
│   ├── loop-state-manager/       # HITL loop state management
│   ├── pkg-ctx/                  # Local-first docs MCP server (FTS5 + git)
│   ├── portail-agents/           # CI agents (drift, spec, fuzz, chore, nullclaw)
│   ├── portail-plugin-sdk/       # Plugin SDK for .vaked plugins
│   └── portail-vaked/            # Plugin registry, CLI, Nix lowering
│
├── tests/                        # Integration + layer + property tests
├── docs/                         # Documentation
├── scripts/                      # Shell scripts
├── benches/                      # Criterion benchmarks
└── creative_tui/                 # Experimental GPU-accelerated TUI (WGSL shaders)
```

### Request Lifecycle

```
Client → axum router → middleware (request ID, logging, metrics, auth, rate-limit)
  ├─ /v1/chat/*     → hooks.inject → gateway.forward → AI upstream (LiteLLM, OpenAI, etc.)
  ├─ /mcp/*         → mcp.proxy → unix socket → Python MCP sidecar
  ├─ /cdn/*         → cdn.lookup → moka → blake3 disk → origin
  ├─ /events/*      → event_log → broadcast channel → SSE
  ├─ /hooks/*       → hook_store CRUD
  ├─ /a2a/*         → task lifecycle (Agent-to-Agent protocol)
  ├─ /a2c/*         → chat API with tools
  ├─ /dns/*         → dns.resolve → DoH → fallback chain
  ├─ /graphql       → async-graphql endpoint
  ├─ /health        → sentinel status
  ├─ /metrics       → Prometheus scrape
  └─ /dashboard     → live health snapshot (JSON)
```

---

## Local Dev Environment

### Prerequisites

| Tool | Required | Notes |
|------|----------|-------|
| Rust 1.85+ | ✅ | Edition 2024, MSRV 1.85 |
| cargo | ✅ | Ships with Rust |
| clang | ✅ | For rocksdb/lmdb bindings |
| Nix | Optional | Flake-based devShell |

### One-Command Setup

```bash
# Option A: Nix (recommended)
nix develop                              # full environment
# or with direnv:
direnv allow                             # auto-activates on cd

# Option B: Cargo-only
cargo check                              # verifies toolchain
rustup component add clippy rustfmt

# Option C: Setup script
bash scripts/contributor-setup.sh        # installs nix + direnv + hooks
```

### Available Task Commands

```bash
task setup         # Full environment bootstrap (run once)
task check         # cargo check (fast feedback)
task test          # Run all tests
task lint          # clippy + fmt check
task ci            # Full CI pipeline (check → lint → test)
task serve         # Start dev server
task coverage      # Generate coverage report (HTML)
task profile       # Profile with samply/dhat
task docs          # Generate and open API docs
task help          # List all tasks
```

### IDE Setup

**.vscode** settings and launch configs are provided:

- **Recommended extensions**: rust-analyzer, even-better-toml, crates, CodeLLDB, markdownlint
- **Debug config**: `portail serve` with breakpoints pre-configured
- **Format on save**: rustfmt auto-formatting

Open `.vscode/extensions.json` and install recommended extensions.

---

## Testing Guide

### Test Suite Layout

| Test type | Location | Count | What it covers |
|-----------|----------|-------|----------------|
| Unit tests | Inline (`#[cfg(test)]`) | 142 | Individual functions, hot-path types |
| Integration | `tests/v0_2_integration.rs` | 15 | Full request lifecycle, auth, rate-limit |
| Layer tests | `tests/layer_tests.rs` | 15 | Middleware layer behavior |
| Property tests | `tests/proptests.rs` | 5 | Invariant-based fuzzing |
| Doc tests | Inline (`/// ``` ```) | 1 | Verified examples |
| **Total** | | **177+** | |

### Running Tests

```bash
# All tests (default)
cargo test

# Fast parallel tests (nextest — install: cargo install cargo-nextest)
cargo nextest run

# Specific test
cargo test test_name              # runs any test matching "test_name"
cargo test v0_2_integration       # just integration suite

# With output
cargo test -- --nocapture

# Single module
cargo test -p loopeng             # test only the loopeng crate

# Coverage (requires cargo-llvm-cov)
cargo llvm-cov                    # terminal report
cargo llvm-cov --open             # HTML report
```

### Adding Tests

1. **Unit tests**: Add `#[cfg(test)] mod tests { ... }` at the bottom of the source file
2. **Integration tests**: Add to `tests/v0_2_integration.rs` — each test gets its own server instance
3. **Property tests**: Add to `tests/proptests.rs` using the `proptest` crate
4. **Coverage**: Run `task coverage` to verify your test covers the new code

### Test Conventions

- Use `anyhow::Result` as return type for ergonomic `?`
- Use `test_utils::test_config()` for config boilerplate
- Integration tests start a real server on a random port
- Tests should be hermetic — no network calls to external services

---

## Code Style & Conventions

### Rustfmt & Clippy

```bash
# Format check (CI enforces this)
cargo fmt --check

# Auto-format
cargo fmt

# Lint (must pass with zero warnings)
cargo clippy --all-targets -- -D warnings
```

### Naming Conventions

| Pattern | Example | Rule |
|---------|---------|------|
| Modules | `snake_case` | `src/gateway/mod.rs` |
| Types | `PascalCase` | `struct AppState`, `enum CacheResult` |
| Functions | `snake_case` | `fn handle_request()` |
| Constants | `SCREAMING_SNAKE` | `MAX_EVENTS` |
| Tests | `snake_case` | `fn auth_no_header_returns_401()` |
| Errors | `PascalCase` | `enum ProxyError` with `thiserror` |

### Key Rules

- **No `unsafe`**: `#![forbid(unsafe_code)]` at crate root — enforced by compiler
- **No `unwrap`/`expect`**: use `anyhow::Context` or proper error handling
- **Edition 2024**: native `async fn in traits`, `unsafe_op_in_unsafe_fn`
- **Allocators**: mimalloc by default, jemalloc feature flag, system alloc cfg
- **Clone is cheap**: Arc for shared state, never clone large data

### Safety Features

- `unsafe_op_in_unsafe_fn` denied
- `unsafe_code` forbidden
- `-D warnings` in CI (zero tolerance)
- `cargo deny` for license + duplicate dep checks
- `cargo audit` for dependency CVEs

---

## PR Process

### Before You Open a PR

1. **Claim the issue** — comment on the issue to avoid duplicate work
2. **Branch naming**: `feature/short-description` or `fix/bug-description`
3. **Commit messages**: follow [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat: new feature`
   - `fix: bug fix`
   - `docs: documentation`
   - `refactor: code restructure`
   - `test: test additions`
   - `chore: maintenance`
4. Prefer atomic commits: one logical change per commit

### PR Checklist

- [ ] `cargo test` passes all tests
- [ ] `cargo clippy --all-targets -- -D warnings` — zero warnings
- [ ] `cargo fmt --check` — formatting matches rustfmt
- [ ] New code has tests
- [ ] Public API has doc comments
- [ ] CHANGELOG.md updated (if user-facing change)

### Review Expectations

- At least 1 maintainer approval required
- All CI checks must pass
- No merge conflicts with `main`
- Review focuses on: correctness, safety, test coverage, documentation

### After Merge

- Squash merge preferred
- Your commit message becomes the merge title
- Branch auto-deleted after merge

---

## Issue Triage

### Issue Labels

| Label | Meaning | Good for new contributors? |
|-------|---------|---------------------------|
| `good first issue` | Bounded, well-scoped, minimal codebase knowledge | ✅ Yes |
| `help wanted` | Need contribution, moderate complexity | ✅ Yes |
| `enhancement` | New feature or improvement | Depends on scope |
| `bug` | Something is broken | Usually yes |
| `documentation` | Docs improvement | ✅ Great starting point |
| `v2.x` | Release tracker issues | Coordination, not coding |
| `v3.0` | AI-native architecture | Advanced |

### How to Pick an Issue

1. Filter by `good first issue` or `help wanted`
2. Read the issue and linked plan docs
3. Comment `.take` or `I'll work on this` to claim it
4. If you have questions, ask in the issue thread

### What Issues Need

- For `enhancement`: describe the implementation approach before coding
- For `bug`: provide reproduction steps
- For `documentation`: link to the file being improved

---

## Release Process

### Versioning

Portail follows [Semantic Versioning](https://semver.org/):

- **PATCH** (0.x.1): Bug fixes, minor changes
- **MINOR** (0.1.0): New features, backward compatible
- **MAJOR** (1.0.0): Breaking changes

Current release: **v2.1.0** (2026-06-27)

### Cutting a Release

1. **Update versions** in all `Cargo.toml` files
2. **Update `CHANGELOG.md`** — consolidate changes under the new version
3. **Verify** — `cargo test && cargo clippy && cargo fmt --check`
4. **Tag & push**:
   ```bash
   git tag v2.1.0
   git push --tags
   ```
5. **CI** builds release binaries, signs with cosign, creates GitHub Release
6. **Publish to crates.io**: `cargo publish` (maintainer access)
7. **Announce**: blog post at pocoo.vaked.dev + social (X/Twitter, Reddit, HN)

### Release Artifacts

- Linux x86_64 + aarch64 binaries
- macOS aarch64 binary
- Docker image at `ghcr.io/peterlodri-sec/portail:latest`
- Homebrew formula (`.rb`)
- AUR package (`PKGBUILD`)
- Nix flake (`.nix`)
- `crates.io` package

---

## For AI Agents

### Supported Agents

Portail's repo is designed for AI agent contributions. Supported agent systems:

- **Claude Code** (Anthropic) — preferred
- **OpenCode** / **Codex** — compatible
- **GitHub Copilot** — compatible
- **CodeWhale** — full support

### Agent Workflow

```bash
# 1. Create a feature branch
git checkout -b agent/feature-name

# 2. Make changes and test
cargo test
cargo clippy --all-targets -- -D warnings

# 3. Commit with agent prefix
git commit -m "agent: description of changes"

# 4. Push and create PR
git push origin agent/feature-name
gh pr create --title "agent: feature name" --body "Description of changes"
```

### Permissions

**Allowed:**
- Push to feature branches
- Create PRs
- Trigger CI on same-repo PRs
- Comment on issues and PRs

**Not allowed:**
- Push to `main` directly
- Push version tags
- Access production secrets
- Modify branch protection rules

### Triggering Agent Builds

```bash
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/peterlodri-sec/portail/dispatches \
  -d '{"event_type": "agent-build", "client_payload": {"agent_id": "claude-code", "feature": "my-feature"}}'
```

### Security Model

- Fork PRs → GitHub-hosted runners (sandboxed)
- Same-repo PRs → Self-hosted runners (trusted)
- Only maintainers can merge to `main` or push tags

---

## CI/CD Pipeline

### Workflow Summary

| Workflow | Trigger | Runner | Duration |
|----------|---------|--------|----------|
| `ci.yml` | Push, PR | Self-hosted + GitHub | ~2 min |
| `release.yml` | Tag push | Self-hosted | ~5 min |
| `docker.yml` | Tag push | Self-hosted | ~3 min |
| `agent-build.yml` | Repository dispatch | Self-hosted | ~3 min |
| `benchmark-gate.yml` | PR label | Self-hosted | ~4 min |
| `e2e.yml` | Push, PR | Self-hosted | ~3 min |
| `chore-bot.yml` | Scheduled | GitHub | ~1 min |

### CI Agents (Advisory, Non-Blocking)

| Agent | Exit | Purpose |
|-------|------|---------|
| complexity | always 0 | Track code complexity metrics |
| drift-detect | always 0 | Capture/replay regression |
| spec-verify | always 0 | Route spec matches golden file |
| fuzz-route | 1 on panic | Crash detection via fuzzing |
| chore-bot | always 0 | Auto-fix imports, formatting |

### Blocking Gates

| Gate | Exit | Purpose |
|------|------|---------|
| clippy | 1 on warning | Code quality |
| test | 1 on failure | Correctness |
| fmt | 1 on mismatch | Formatting |

---

## Troubleshooting

### Common Issues

**Problem**: `cargo test` fails on macOS with code signing errors
```
Fix: codesign --force --sign - target/debug/portail
```

**Problem**: `nix develop` fails on macOS
```
Fix: nix --extra-experimental-features 'nix-command flakes' develop
```

**Problem**: `cargo clippy` reports unrelated warnings
```
Fix: rustup update && cargo clippy --all-targets -- -D warnings
```

**Problem**: Tests hang or timeout
```
Fix: Tests bind to random ports; check no port conflicts. cargo test --test-threads=1
```

**Problem**: RocksDB build fails
```
Fix: Install clang (macOS: xcode-select --install, Linux: apt install clang)
```

### Getting Help

- Open a [GitHub Discussion](https://github.com/peterlodri-sec/portail/discussions)
- File a [Bug Report](https://github.com/peterlodri-sec/portail/issues/new?template=bug_report.md)
- Chat on our [community server](https://chat.vaked.dev)
- Security issues: see [`SECURITY.md`](SECURITY.md)

---

## License

Portail is MIT-licensed. See [`LICENSE`](LICENSE) for details.
