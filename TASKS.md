# Portail — Task & Decision Log

## Architecture
- **Three subsystems** behind one axum router: AI Gateway, MCP Gateway, CDN Cache
- **Hybrid Rust + Python**: Rust handles proxying, Python sidecar handles MCP tool execution via LiteLLM
- **Config**: TOML file + CLI flags + env vars (CLI overrides TOML)
- **State**: `Arc<AppState>` with `RwLock<Config>` for SIGHUP reloadable config
- **Metrics**: `metrics` crate + `metrics-exporter-prometheus` on `/metrics`
- **Logging**: Structured JSON via `tracing-subscriber` with `TraceLayer` access logs

## Build & Release Pipeline

### Dev profile
```toml
[profile.dev]
opt-level = 0
debug = true
split-debuginfo = "unpacked"
```
Uses `mold` linker on Linux, `rust-analyzer` for IDE support.

### Release profile
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = "symbols"
```

### CI pipeline
`.github/workflows/ci.yml` — runs on push/PR to main:
1. `cargo check` (fastest)
2. `cargo test` (all 20+ unit + integration)
3. `cargo clippy -- -D warnings`
4. `cargo fmt --check`
5. `cargo bench` (criterion, compared against main baseline)
6. `nix flake check` (NixOS module + package)

### Caching
- `actions/cache` for `~/.cargo/registry`, `~/.cargo/git`, `target/`
- `sccache` for Rust compilation caching
- GitHub Actions `rust-cache` action for per-branch keyed caches

## NixOS Module

### Structure
```nix
flake.nix → nixosModules.default → {
  services.portail = {
    enable, enableAiGateway, enableMcp, enableCdn,
    listen, cacheDir, cacheSize,
    aiUpstream, cdnOrigin, natsUrl,
    cdnDomains, mcpConfig, openFirewall
  };
}
```

### Hardening
- `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`
- Dedicated `portail` user/group
- `RuntimeDirectoryPreserve=yes` for Unix socket
- `OOMScoreAdjust=-500` (low OOM kill priority)
- `MemoryMax`, `TasksMax` resource limits
- `Restart=on-failure` with `RestartSec=5s`

### Build
```nix
rustPlatform.buildRustPackage {
  cargoBuildFlags = "--bin portail";
  CARGO_PROFILE_RELEASE_LTO = "fat";
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
}
```

## Key Decisions & Trade-offs

| Decision | Rationale | Alternative |
|----------|-----------|-------------|
| `RwLock<Config>` over `arc-swap` | Simpler, config reads not on hottest path | `arc-swap` lock-free reads |
| `OnceLock` for HTTP clients | No deps, negligible init cost | `lazy_static` / `once_cell` |
| Own `metrics` middleware vs tower-http | Full control over labels | `tower-http` metrics (less flexible) |
| Separate `portail-mon` binary | 0 deps beyond what portail already has | `ratatui` TUI (heavy dep) |
| Python sidecar over Unix socket | Leverages LiteLLM ecosystem | Pure Rust MCP (more work) |

## TODOs

- [ ] Publish `v0.1.0` to crates.io
- [ ] Add Dockerfile + multi-arch ghcr.io builds
- [ ] OpenTelemetry export (OTLP)
- [ ] Headroom compression for cache storage
- [ ] Rate limiting (token bucket per key/IP)
- [ ] Auth middleware (API key classification tiers)
- [ ] Config hot-reload for all subsystems (not just upstream URLs)

## Related Repos
- `nix-base` — NixOS config that imports portail module on dev-cx53
- `__cdn-archive` — deprecated Colmena NGINX CDN (replaced by portail CDN)
