# GitHub Copilot Coding Agent Instructions

## Project Overview

Portail is a unified proxy/gateway for AI services, MCP tools, and CDN caching. Built in Rust with zero-copy I/O, SIMD-optimized hashing, and a live TUI dashboard.

## Architecture

- **Layer 7 proxy** — axum + tokio + reqwest
- **Two-tier cache** — Moka (memory) + Redis (network-wide)
- **Agent protocols** — A2A (Agent-to-Agent), A2C (Agent-to-Consumer)
- **Performance engines** — eBPF, io_uring, DPDK, hyper (optional, feature-gated)

## Key Files

```
src/
├── main.rs          # Entry point, CLI dispatch
├── lib.rs           # AppState, module declarations
├── proxy.rs         # HTTP routing, middleware
├── gateway.rs       # AI upstream forwarding
├── cdn.rs           # Cache (Moka + disk)
├── events.rs        # Event log + SSE
├── hooks.rs         # Prompt injection
├── sentinel.rs      # Health monitoring
├── dns.rs           # DNS + DoH
├── a2a.rs           # Agent-to-Agent
├── a2c.rs           # Agent-to-Consumer
├── mcp.rs           # MCP sidecar proxy
├── cli/             # CLI commands
├── plugins/         # Built-in plugins
└── nullclaw.rs      # Network-native agent
```

## Conventions

### Code Style
- Use `anyhow::Result` for error handling
- Use `rustc_hash::FxHashMap` for hot paths
- Use `std::sync::RwLock` for shared state (with `unwrap_or_else`)
- Use `Arc<T>` for shared ownership across async tasks

### Testing
- Run `cargo test` for all tests
- Run `cargo clippy -- -D warnings` for linting
- Use `OnceLock` for shared metrics handles in tests
- All tests must pass before merge

### Security
- Never log secrets or API keys
- Use `secrecy::SecretString` for sensitive values
- Validate all user input
- Use `tower_http::limit::RequestBodyLimitLayer` for body size limits

### Performance
- Use `mimalloc` as global allocator
- Use `blake3` for hashing (SIMD-optimized)
- Use `tokio::sync::broadcast` for event streaming
- Use `axum::extract::State` for shared state

## Build & Test

```bash
# Debug build
cargo build

# Release build (LTO + strip)
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt --check
```

## Common Tasks

### Add a new API endpoint
1. Add route in `src/proxy.rs` `build_router()`
2. Create handler function
3. Add to `AppState` if needed
4. Add tests

### Add a new plugin
1. Create `src/plugins/your_plugin.rs`
2. Add to `src/plugins/mod.rs`
3. Add store to `AppState` in `src/lib.rs`
4. Wire into `src/main.rs` initialization
5. Add routes in `src/proxy.rs`

### Add a new CLI command
1. Add variant to `Commands` enum in `src/cli/mod.rs`
2. Create handler in `src/main.rs` `dispatch_cli()`
3. Add help text

## Dependencies

- `axum` — HTTP framework
- `tokio` — Async runtime
- `reqwest` — HTTP client
- `moka` — In-memory cache
- `redis` — Redis client
- `blake3` — Hashing
- `clap` — CLI parsing
- `ratatui` — TUI framework
- `serde` — Serialization
- `tracing` — Logging
