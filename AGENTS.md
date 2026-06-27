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

---

## Essential Commands

### Development Workflow

```bash
# Check syntax (fast)
cargo check

# Build debug binary
cargo build

# Build release binary with LTO
cargo build --release

# Run all tests
cargo test

# Run nextest (faster)
cargo nextest run

# Lint (clippy + fmt)
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check

# Format code
cargo fmt

# Run benchmarks
cargo bench

# Clean build artifacts
cargo clean
```

### Nix Development Environment

```bash
# Enter Nix shell (includes toolchain, crane, mold linker)
nix develop

# Build binary via Nix
nix build .#portail

# Run one-command start
nix run github:peterlodri-sec/portail -- serve

# Verify flake checks pass
nix flake check --impure

# Clean Nix garbage (keeps last 7 days)
nix-collect-garbage --delete-older-than 7d
```

### Taskfile Commands (requires `task`)

```bash
# Show available tasks
task --list

# Build
task build
task build-release

# Test
task test
task test-fast

# Lint
task lint

# Format
task format

# Full CI pipeline
task ci

# Clean
task clean
task clean-all
task clean-nix
```

### Justfile Commands (requires `just`)

```bash
# Build
just build
just release

# Test
just test
just nextest

# Lint
just lint
just clippy
just format

# Watch mode
just watch

# Security audit
just audit
just deny
just outdated

# Full CI
just ci

# Nix
just nix-check
just nix-build

# Docker
just docker-build
just docker-run
just docker-slim

# Publish
just login
just publish-dry
just publish
```

### CLI Subcommands

```bash
# Start server
portail serve

# Interactive dashboard
portail

# Check status
portail status

# View events
portail events

# Manage hooks
portail hooks list
portail hooks add --hook '{"id":"h1","match_path":"/chat","inject":"prepend","content":"Be helpful"}'

# Learn networking
portail learn dns
portail learn tls
portail learn tcp

# Setup domain + certificates
portail setup --domain portail.example.com

# Analyze code complexity
portail complexity

# Generate docs
portail docs --open

# Install binary
portail install
```

---

## Code Patterns & Conventions

### Module Structure

Each top-level module in `src/` follows a consistent pattern:

1. **Configuration-driven**: Most modules accept a config struct from `config.rs`
2. **Arc-wrapped shared state**: `AppState` holds `Arc<T>` for all cross-cutting concerns
3. **Tokio async**: All I/O-bound operations are `async fn`
4. **Error handling**: `anyhow::Result<T>` for high-level APIs, specific errors internally

### Naming Conventions

- **Types**: PascalCase (`Config`, `HookStore`, `CacheManager`)
- **Functions**: snake_case (`load_config`, `inject_hooks`, `lookup_cache`)
- **Modules**: lowercase with underscores (`config_watcher`, `events_stream`)
- **Constants**: UPPER_SNAKE_CASE (`MAX_EVENTS`, `DEFAULT_PORT`)
- **Environment vars**: PORTAIL_<FEATURE>_<SETTING>

### Error Handling

```rust
// High-level: propagate with anyhow
pub fn handle_request() -> anyhow::Result<Response> {
    let config = self.config.read().unwrap();
    // ...
}

// Low-level: return specific error types when useful
pub fn parse_hook(json: &str) -> Result<Hook, HookParseError> {
    // ...
}

// Contextual errors use `Context` trait
let config = fs::read_to_string(path).context("failed to read config")?;
```

### Async Patterns

```rust
// Spawn background tasks
tokio::spawn(async move {
    // long-running loop
});

// Shared mutable state: Arc<Mutex<T>> or Arc<RwLock<T>>
pub struct AppState {
    pub config: RwLock<Config>,
    pub hooks: Arc<HookStore>,
}

// Broadcast channels for pub/sub
let (tx, rx) = tokio::sync::broadcast::channel(256);
```

### Configuration Pattern

```toml
# portail.toml structure
listen = "0.0.0.0:8787"

[feature_name]
enabled = true
key = "value"

[[feature_list]]
name = "item1"
# ...
```

Load with figment:
```rust
let config = Config::load("portail.toml").context("failed to load config")?;
```

---

## Testing Approach

### Test Categories

1. **Unit tests** (`#[cfg(test)]` modules within each source file)
2. **Integration tests** (`tests/v0_2_integration.rs`)
3. **Layer tests** (`tests/layer_tests.rs`)
4. **Property-based tests** (`tests/proptests.rs` using proptest)

### Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_function_name

# Integration tests only
cargo test --test v0_2_integration

# Proptest only
cargo test --test proptests
```

### Test Patterns

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        let result = compute(42);
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_async() {
        let data = fetch_data().await;
        assert!(!data.is_empty());
    }
}
```

---

## Important Gotchas

### Allocator Selection

Portail ships with multiple allocators selected at compile time:

```bash
# Default: mimalloc (fast general-purpose)
cargo build

# jemalloc (better for high-concurrency)
cargo build --features jemalloc

# System allocator (for comparison)
cargo build --cfg portail_system_alloc
```

⚠️ **Don't mix allocators** in benchmarks—always pin one.

### Nix vs Cargo

- **Nix**: Reproducible builds, split dependency cache, production binaries
- **Cargo**: Fast iteration during development

Use `nix develop` to get the exact toolchain specified in `flake.nix`.

### Feature Flags

Common features in `Cargo.toml`:
- `jemalloc` — Use tikv-jemallocator instead of mimalloc
- `aws-lc-rs` — Use AWS-LC for TLS (FIPS-compliant)

Check features before building:
```bash
cargo metadata --format-version 1 | jq '.packages[] | select(.name=="portail") | .features'
```

### Hot Path Optimizations

Critical performance paths use specialized types:
- `FxHashMap` (rustc-hash) for event metadata
- `BoundedMeta` (max 16 entries, key≤128B, val≤512B)
- `blake3` for cache key hashing (SIMD-accelerated)

⚠️ Don't replace these without benchmarking impact.

### CI Agent Behavior

All advisory CI agents (complexity, drift-detect, spec-verify, fuzz-route) **never block CI**. They:
- Always exit 0 unless they find a critical bug (fuzz-route panics)
- Post reports as PR comments via GitHub API
- Are safe to modify/improve without breaking CI

---

## Current Development State (v2.2)

**Issue #28**: Nix Shell + Nushell + OSS Release + Blog Post

### TODO List

- [ ] `nix flake check` — CI verification script
- [ ] `nix run . -- serve` — one-command server start (already works)
- [ ] Nushell tab completions (`portail completions nushell`)
- [ ] Homebrew formula (`portail.rb`)
- [ ] AUR package (`PKGBUILD`)
- [ ] Docker multi-arch (linux/amd64 + linux/arm64)
- [ ] Blog post at pocoo.vaked.dev — "Your AI Infrastructure's Nervous System"
- [ ] Social posts: X/Twitter, Reddit, Hacker News

**Target**: v2.2 — Jul 2026

### Related Issues

- #27, #29, #30, #31, #33, #34, #35
- #1 super devnote (HUMAN ONLY)

See [`LOOP_STATE.md`](LOOP_STATE.md) for full roadmap.

---

## Debugging Tips

### Enable Verbose Logging

```bash
RUST_LOG=debug,portail=trace ./target/release/portail serve
```

### Inspect Runtime State

```bash
# Prometheus metrics
curl http://localhost:8787/metrics

# Health checks
curl http://localhost:8787/healthz
curl http://localhost:8787/livez
curl http://localhost:8787/readyz

# Events stream
curl http://localhost:8787/events/stream
```

### Nix Troubleshooting

```bash
# Check if in Nix shell
echo $IN_NIX_SHELL  # should be "1" or "pure"

# Rebuild Nix store
nix build .#portail --no-link

# Garbage collect old generations
nix-collect-garbage -d
```

---

## External Resources

- **GitHub**: https://github.com/peterlodri-sec/portail
- **Docs**: https://pocoo.vaked.dev (blog)
- **Issues**: https://github.com/peterlodri-sec/portail/issues
- **Crates.io**: https://crates.io/crates/portail (planned)

---

*Last updated: 2026-06-27*
