# Changelog

## v0.2.0 (unreleased)

### Features
- **A2A Protocol** — Google Agent-to-Agent: agent cards, task lifecycle, message streaming
- **A2C Interface** — Agent-to-Consumer: human-facing chat API with tool use
- **TUI Dashboard** — Live network visualization with sparklines, keyboard navigation
- **CLI Subcommands** — `portail status/events/hooks/config/cache/health`
- **First-class agent support** — Inbox/outbox patterns for agent communication

### Infrastructure
- Self-hosted runners on dev-cx53 (x86_64-linux)
- Updated all GitHub Actions to latest versions
- Added DESIGN.md with architecture documentation

### Improvements
- Simplified Config loading (path-based instead of CLI args)
- Fixed layer violations between modules
- Added Default implementations for all public types
- Clippy clean with `-D warnings`

## v0.1.0 (2026-06-26)

### Features
- AI Gateway — streaming proxy to LiteLLM upstream
- MCP Gateway — Unix-socket framed protocol to Python sidecar
- CDN Cache — two-tier moka in-memory + blake3-filesystem
- Prometheus metrics — http_requests_total, per-subsystem counters
- Structured access logs — JSON output via TraceLayer
- Request ID middleware — x-request-id injection
- SIGHUP config reload — zero-downtime upstream swaps
- Agent event log — ring buffer + broadcast channel, SSE streaming
- Sentinel watcher — 30s health checks and CDN scrub monitoring
- Hook injection — per-message prompt prepend/append and per-event metadata injection

### Infrastructure
- Binary: `release.yml` — 3 targets, UPX compressed, cosign signed
- Container: `docker.yml` — multi-arch, ghcr.io, cosign signed, SBOM
- Package: `crates.io` — cargo publish
- Nix: flake-parts + rust-overlay, nixosModules, hardened systemd

### Hardware
- mimalloc global allocator
- rustc-hash FxHashMap for hot paths
- blake3 with native SIMD
