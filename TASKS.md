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

### Release distribution pipeline (`release.yml`)
Triggered on `v*` tags. Performs the following for each target:

1. **Build** — `cargo build --release --target $target --bin portail`
2. **UPX** — `upx --best --lzma` compresses the binary (60-80% size reduction)
3. **Checksum** — `sha256sum` of the compressed binary
4. **Cosign (keyless)** — `cosign sign-blob` using GitHub OIDC:
   - Signs the compressed binary → `.sig` + `.pem`
   - Signs the checksum file → `.sha256.sig` + `.sha256.pem`
5. **GitHub Release** — uploads all assets + combined `SHA256SUMS` (signed)
6. **crates.io** — `cargo publish` via `CARGO_REGISTRY_TOKEN`

**Targets:**
- `x86_64-unknown-linux-gnu` (UPX: --best --lzma)
- `aarch64-unknown-linux-gnu` (UPX: --best --lzma)
- `aarch64-apple-darwin` (UPX: --best)

**Verification:** cosign signatures are verified before creating the release.

### Nix build (`nix/package.nix`)
- Runs `upx --best --lzma` in `postInstall` phase
- LTO, codegen-units=1, strip via `CARGO_PROFILE_RELEASE_*` env vars
- Builds only `--bin portail` (not portail-mon or benches)

### CI pipeline
`.github/workflows/ci.yml` — runs on push/PR to main:
1. `cargo check` (fastest)
2. `cargo test` (all 20+ unit + integration)
3. `cargo clippy -- -D warnings`
4. `cargo fmt --check`
5. `cargo bench` (criterion, compared against main baseline)
6. `nix flake check` (NixOS module + package)

### Docker image (`Dockerfile`)
Multi-stage build:
1. **Builder** — `rust:1.85-slim-bookworm`, installs `upx-ucl`, builds with LTO/fat/codegen-units=1, UPX `--best --lzma`
2. **Runtime** — `gcr.io/distroless/cc-debian12`, single binary, non-root user `1000:1000`
3. **SBOM + provenance** enabled via Docker Buildx

### ghcr.io publishing (`docker.yml`)
Triggered on `v*` tags:
1. Set up QEMU + Buildx for multi-arch
2. Login to ghcr.io via `GITHUB_TOKEN`
3. Generate tags: `vX.Y.Z`, `vX.Y`, `vX`, `edge`
4. Build + push `linux/amd64,linux/arm64` with GHA cache
5. Cosign sign image (keyless via OIDC)
6. Verify signature

### Caching
- `actions/cache` for `~/.cargo/registry`, `~/.cargo/git`, `target/`
- `sccache` for Rust compilation caching
- GitHub Actions `rust-cache` action for per-branch keyed caches
- Docker: `type=gha` cache mode for Buildx layer caching

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

- [x] v0.1.0 release prep: CHANGELOG, cargo-credential-pass, just login/publish
- [ ] **Publish v0.1.0** (see [RELEASE.md](./RELEASE.md))
  - [ ] `cargo login` (uses cargo-credential-pass, stores in macOS Keychain)
  - [ ] `nix flake check` (ensure CI passes)
  - [ ] `git tag v0.1.0 && git push --tags` (triggers release + docker workflows)
  - [ ] Verify GitHub Release, Docker push, crates.io publish
- [ ] OpenTelemetry export (OTLP)
- [ ] Headroom compression for cache storage
- [ ] Rate limiting (token bucket per key/IP)
- [ ] Auth middleware (API key classification tiers)
- [ ] Config hot-reload for all subsystems (not just upstream URLs)

## Related Repos
- `nix-base` — NixOS config that imports portail module on dev-cx53
- `__cdn-archive` — deprecated Colmena NGINX CDN (replaced by portail CDN)
