# Contributing

Thank you for your interest in Portail! This document covers the
development workflow and conventions.

## Getting started

```bash
# Clone and build
git clone https://github.com/peterlodri-sec/portail
cd portail
cargo build

# Run tests
cargo test

# Lint
cargo clippy -- -D warnings
cargo fmt --check
```

## Project structure

```
portail/
├── src/
│   ├── main.rs          # Entry point, subsystem wiring
│   ├── config.rs        # CLI + TOML + env config
│   ├── proxy.rs         # Axum router, route handlers
│   ├── cdn/             # CDN cache subsystem
│   │   ├── cache.rs     # Two-tier (moka → filesystem) cache
│   │   ├── mod.rs       # Request handler, origin fetch
│   │   └── purge.rs     # NATS-based cache invalidation
│   ├── gateway/         # AI Gateway subsystem
│   │   └── mod.rs       # Upstream proxy with hop-by-hop stripping
│   └── mcp/             # MCP Gateway subsystem
│       └── mod.rs       # Unix socket framed protocol, sidecar lifecycle
├── plugins/
│   └── portail-mcp/     # Python MCP sidecar
│       └── portail_mcp/
│           ├── gateway.py  # LiteLLM MCPServerManager wrapper
│           └── server.py   # Unix socket ASGI server
└── nix/
    ├── module.nix       # NixOS module
    ├── package.nix      # Rust package derivation
    └── mcp-plugin.nix   # Python package derivation
```

## Pull request process

1. Fork the repo and create a feature branch from `main`.
2. Make your changes — include tests where applicable.
3. Run `cargo test` and `cargo clippy`.
4. Submit a PR with a clear description of what and why.
5. CI must pass before merge.

## Coding conventions

- **No unsafe code** — Portail is built entirely on safe Rust principles.
- **No comments on business logic** — code should be self-documenting.
- **Async first** — all I/O is async via tokio.
- **Type-driven** — use strong types for config, cache keys, etc.
- **Tests in same file** — unit tests live in `#[cfg(test)] mod tests` blocks alongside the code they test.

## Release process

Maintainers trigger releases by tagging `v0.x.y` on `main`. CI publishes
to crates.io and the GitHub release page.
