# Portail v2.1 + v2.2 — Bulletproof CLI + Nix Shell

## v2.1 — Bulletproof CLI (2 scopes)

**Goal**: Every CLI command works offline AND against a running server,
with clear error messages, tab completion, and proper testing.

### Scope 1: CLI Reliability & UX (1 week)

#### 1.1 Offline-First Commands
Commands that should work without a running server:
- `portail init` — already works
- `portail config show` — already works (reads file)
- `portail config validate` — already works
- `portail doctor` — already works
- `portail docs` — already works
- `portail learn` — already works
- `portail amberify` — already works

#### 1.2 Online Commands that Need Server
These need server connection. Add graceful failure with clear message:
- `portail status` — ✅ already warns if server not running
- `portail events` — ✅ already warns
- `portail hooks` — ✅ already warns
- `portail health` — ✅ already warns
- `portail cache stats` — ✅ already warns

#### 1.3 New CLI Features
- [ ] `portail --man` / `portail --help-long` — full man-page style help
- [ ] `portail --json` — force JSON output for all commands (not just complexity)
- [ ] `portail --quiet` — suppress all output except errors (good for CI)
- [ ] Shell completions: `portail completions bash|zsh|fish|nushell`
- [ ] `portail serve --daemon` — detach and run in background
- [ ] `portail serve --pid-file` — write PID for process management
- [ ] `portail serve --watch` — auto-restart on config change (uses watcher)
- [ ] `portail stop` — graceful shutdown via SIGTERM to running instance
- [ ] `portail restart` — stop + start
- [ ] CLI integration tests for every single command (test offline AND online)

#### 1.4 Error Handling
- [ ] All CLI errors use `anyhow` with contextual messages
- [ ] No raw panic messages in user-facing output
- [ ] Connection refused → "Is portail running? Start with: portail serve"
- [ ] Permission denied → clear message + suggest sudo or alt path
- [ ] Config parse error → show exact line + column of the issue

#### 1.5 CLI Testing
- [ ] `tests/cli_offline.rs` — test every offline command
- [ ] `tests/cli_online.rs` — test every online command against running server
- [ ] Test help output is well-formed
- [ ] Test JSON output flag on all commands
- [ ] Test error messages for common failure scenarios

**Target**: 185+ tests (175 + ~10 CLI tests)

---

### Scope 2: Config + Daemon UX (1 week)

#### 2.1 Config Improvements
- [ ] `portail config edit` — opens $EDITOR on portail.toml
- [ ] `portail config path` — prints config file path
- [ ] `portail config diff` — show diff between current and saved config
- [ ] `portail config migrate` — upgrade config from older version format
- [ ] `portail config backup` — create timestamped backup

#### 2.2 Daemon Mode
- [ ] `portail serve --daemon` — fork to background
- [ ] PID file management (write, read, cleanup)
- [ ] `portail stop` — graceful shutdown (SIGTERM, wait, SIGKILL)
- [ ] `portail restart` — stop + start, preserves config
- [ ] `portail logs` — tail the portail log file
- [ ] systemd integration test

#### 2.3 Tab Completion
```bash
portail completions bash  > /usr/local/share/bash-completion/completions/portail
portail completions zsh   > /usr/local/share/zsh/site-functions/_portail
portail completions fish  > ~/.config/fish/completions/portail.fish
portail completions nushell > ~/.config/nushell/completions/portail.nu
```

#### 2.4 CLI Polish
- [ ] Colors: green for success, yellow for warnings, red for errors
- [ ] Progress indicators for long-running commands
- [ ] Consistent formatting across all commands
- [ ] `--verbose` flag shows detailed output

**Target**: 190+ tests

---

## v2.2 — Nix Shell + Nushell + Blog (1 week)

### Scope 1: Nix Dev Shell Improvements

#### 1.1 flake.nix Enhancements
- [ ] `nix develop` — improved dev shell with all tools
- [ ] `nix build .#portail` — builds release binary (already exists)
- [ ] `nix run . -- serve` — one-command server start (already exists)
- [ ] `nix flake check` — CI verification (already exists)
- [ ] Add `shellHook` with portail-specific aliases
- [ ] Cache pre-builds for faster CI

#### 1.2 NixOS Module Improvements
- [ ] Systemd hardening: `ProtectSystem=strict`, `NoNewPrivileges=yes`
- [ ] Firewall rules for port 8787
- [ ] Log rotation configuration
- [ ] Secrets management (age/sops integration)

#### 1.3 Nushell Integration
- [ ] Nushell tab completions (`portail completions nushell`)
- [ ] Nushell dev shell (`nix develop .#nushell`)
- [ ] Example configs in nushell format (.nu files)
- [ ] `shell.nix` for non-flake users

### Scope 2: OSS Release + Blog Post

#### 2.1 Release Checklist
- [ ] Tag v2.0.0 → release CI builds binaries
- [ ] crates.io: `cargo publish` (auto via release workflow)
- [ ] Homebrew formula: `brew install portail`
- [ ] AUR package: `yay -S portail`
- [ ] Docker: `docker pull ghcr.io/peterlodri-sec/portail:latest`
- [ ] Nix: `nix run github:peterlodri-sec/portail/v2.0.0 -- serve`

#### 2.2 Blog Post (pocoo.vaked.dev)
Title: **"Portail: Your AI Infrastructure's Nervous System"**

Sections:
1. What is Portail? (single binary, 5-minute quickstart)
2. Why we built it (cost, fragmentation, security, visibility)
3. Technical deep-dive (Rust, zero-copy, SIMD, 174 tests)
4. Architecture diagram (from docs/architecture/NETWORK_DESIGN.md)
5. E2E scenarios (from docs/E2E_SCENARIOS.md)
6. CI pipeline (7 advisory agents + self-hosted E2E)
7. v2.0.0 changelog highlights
8. Roadmap: v2.1 (bulletproof CLI), v2.2 (Nix + Nu), v3.0 (AI-native)
9. Call to action: cargo install portail, star the repo

#### 2.3 Social Posts
- X/Twitter: thread with key stats, GIF of TUI dashboard
- Reddit: r/rust + r/selfhosted
- Hacker News: "Show HN: Portail — Self-hosted AI gateway in Rust"
- Discord: Rust community, Nix community

---

## Version Roadmap Update

```
v2.0.0 [SHIPPED]  2026-06-26  first production-stable (174 tests)
v2.1   [NEXT]     2026-07-03  bulletproof CLI: daemon mode, completions, CLI tests
v2.2              2026-07-10  Nix shell + nushell + OSS release + blog post
v2.3              2026-07-17  TLS (Let's Encrypt), deploy guide, load testing
v3.0              2026-08-01  AI-native: function calling, prompt versioning, cost attribution
```
