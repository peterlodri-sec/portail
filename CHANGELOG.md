# Changelog

## v0.1.0 (unreleased)

### Features
- AI Gateway — streaming proxy to LiteLLM upstream, hop-by-hop header stripping, x-forwarded-for
- MCP Gateway — Unix-socket framed protocol to Python sidecar, zero-copy encode/decode
- CDN Cache — two-tier moka in-memory + blake3-filesystem, NATS invalidation, configurable domains
- Prometheus metrics — http_requests_total, http_request_duration_seconds, per-subsystem counters
- Structured access logs — JSON output via TraceLayer with method/uri/status/latency
- Request ID middleware — x-request-id injection and preservation
- SIGHUP config reload — zero-downtime upstream swaps via RwLock<Config>
- Agent event log — ring buffer + broadcast channel, POST/GET/SSE endpoints
- Sentinel watcher — 30s health checks and CDN scrub monitoring published as events
- Hook injection — per-message prompt prepend/append and per-event metadata injection, CRUD API
- portail-mon — animated ASCII stream dashboard (zero extra deps)

### Infrastructure
- Binary: `release.yml` — 3 targets, UPX compressed, cosign keyless signed, GitHub Release
- Container: `docker.yml` — multi-arch (linux/amd64 + arm64), ghcr.io, cosign signed, SBOM
- Package: `crates.io` — cargo publish via CARGO_REGISTRY_TOKEN
- Nix: flake-parts + rust-overlay, 3 packages, nixosModules, hardened systemd services

### Hardware
- mimalloc global allocator
- rustc-hash FxHashMap for hot paths
- blake3 with native SIMD (SSE2/AVX2/NEON)
