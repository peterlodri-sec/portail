# Portail Loop State — v0.2 → v1.4

**Principle:** CI agents are advisory only (pass-through). They report, recommend, never block. CI gates are opt-in.

```
LOOP: plan → implement → test → review → ship → repeat
STATE:  v1.0 shipped (sensible defaults, init wizard, A2A interop, 144 tests)
NEXT:   v1.1 self-healing config
OPEN:   #24 integration matrix (Turso pending), #1 super devnote (HUMAN ONLY)
```

---

## v0.2 — Production Hardening                  ✅ SHIPPED
- Rate limiting (token bucket, 429 + Retry-After)
- Auth middleware (API key + JWT, bypass list)
- Persistent event store (SQLite, retention, JSON export)
- OTLP trace export (gRPC to Jaeger/Tempo)
- 12 ghost routers wired, 28 orphaned endpoints recovered
- godfather + nullclaw background runners spawned
- 131 tests (15 integration, 101 unit, 15 layer)

---

## v0.3 — Complexity Bot (advisory only)        ✅ SHIPPED
**Target:** 2026-07-01  **Effort:** 1 day

- Refactor `cli/complexity.rs`: never exit with non-zero
- Output format: TOML report, JSON for machines
- CI mode: `--ci` flag writes report to file, always exits 0
- Report key: per-function Big-O, total project complexity score
- Integration: GitHub Actions step that posts comment on PR
- **Rule:** `complexity-enforcer` is a reporter, not a gate

## v0.4 — Drift Detect (CI agent 1)            ✅ SHIPPED
**Target:** 2026-07-08  **Effort:** 3 days

- `portail drift-detect` CLI subcommand
- Capture mode: record real request/response pairs → compressed `.drift` files
- Replay mode: send recorded requests to proxy, compare SHA-256 of responses
- Diff report: which endpoints changed behavior, by how much
- CI integration: `gh pr comment` with drift report
- **Rule:** advisory only — posts report, never fails CI

## v0.5 — Spec Verify (CI agent 2)              ✅ SHIPPED
**Target:** 2026-07-15  **Effort:** 3 days

- `portail spec-verify` CLI subcommand
- Generate: introspect `Router` → OpenAPI 3.1 JSON via `utoipa` or manual walker
- Golden file: `spec.openapi.json` committed to repo
- Diff mode: compare generated vs golden, report additions/removals/changes
- CI integration: `gh pr comment` with spec diff
- **Rule:** advisory only — posts diff, never fails CI

## v0.6 — Fuzz Route (CI agent 3)               ✅ SHIPPED
**Target:** 2026-07-22  **Effort:** 3 days

- `portail fuzz-route` CLI subcommand
- Feed fuzzed HTTP to every registered route
- Assert: no panics, no 500s on malformed input, proper error codes
- Property: "proxy must not crash on any input"
- CI integration: `gh pr comment` with crash report
- **Rule:** non-zero exit only if a panic/crash is detected (critical bug)

---

## v1.0 — One-Command Gateway (DX)              ✅ SHIPPED
**Target:** 2026-06-26  **Effort:** 1 day

- `nix run github:peterlodri-sec/portail -- serve` is production-ready
- Sensible defaults: rate limiting (30 burst), auth disabled, OTLP off
- Config-less startup: all features work without a config file
- `portail init` wizard: generates portail.toml interactively
- Verified on: x86_64-linux, aarch64-linux, aarch64-darwin
- CI: Nix flake check + binary smoke test on all 3 platforms
- A2A WebSocket /a2a/ws route wired (was dead code)
- 6 new A2A tests: 3 JSON serialization + 3 HTTP integration (144 total)
- AgentGateway interop complete — A2A spec compliance verified

## v1.1 — Self-Healing Config (IX)              🚧 PLANNED
**Target:** 2026-08-08  **Effort:** 3 days

- `inotify` (Linux) / `kqueue` (macOS) config file watcher
- Auto-reload on file change (no SIGHUP needed)
- Validate before apply: parse new config, if invalid → keep old, log error
- Config versioning: store last N valid configs, rollback command
- TUI indicator: green dot = config healthy, red = last reload failed

## v1.2 — Progressive Disclosure TUI (UX)       🚧 PLANNED
**Target:** 2026-08-15  **Effort:** 4 days

- Live overlays on existing TUI dashboard:
  - Cache-hit rate sparkline (60s window)
  - Rate-limit exhaustion counter per key
  - Auth-failure tally per endpoint
  - OTLP trace sampling rate
- Alert mode: when something breaks, highlight + show one-line fix
- Keyboard shortcuts: `r` = reload config, `c` = clear alerts, `q` = quit
- Works in 80x24 terminal minimum

## v1.3 — Polish & Docs                         🚧 PLANNED
**Target:** 2026-08-22  **Effort:** 2 days

- 90%+ test coverage target (currently ~78%)
- `portail docs` generates full API reference + architecture guide
- CONTRIBUTING.md updated with all CLI agent workflows
- CHANGELOG.md consolidated for v1.0 release
- Performance benchmark baseline published

## v1.4 — Release                               🚧 PLANNED
**Target:** 2026-09-01  **Effort:** 1 day

- Tag `v1.4.0`
- All CI green (131+ tests, advisory agents posting comments)
- crates.io publish
- Homebrew formula, AUR package, Docker multi-arch
- Release blog post

---

## CI Agent Policy

| Agent | Blocks CI? | Exit code | Output |
|-------|-----------|-----------|--------|
| complexity (v0.3) | ❌ never | always 0 | TOML report → PR comment |
| drift-detect (v0.4) | ❌ never | always 0 | diff report → PR comment |
| spec-verify (v0.5) | ❌ never | always 0 | spec diff → PR comment |
| fuzz-route (v0.6) | ⚠️ only on panic | 1 on crash, 0 otherwise | crash report → PR comment |
| clippy (existing) | ✅ always | 1 on warning | inline annotations |
| test (existing) | ✅ always | 1 on failure | inline annotations |

---

## Version Bump Schedule

```
v0.2  [SHIPPED]  2026-06-26  rate-limit, auth, store, otlp
v0.3  [SHIPPED]  2026-06-26  complexity advisory
v0.4  [SHIPPED]  2026-06-26  drift-detect
v0.5  [SHIPPED]  2026-06-26  spec-verify
v0.6  [SHIPPED]  2026-06-26  fuzz-route, WebSocket, GraphQL
v1.0  [SHIPPED]  2026-06-26  one-command Nix gateway
v1.1  [NEXT]     2026-07-01  self-healing config
v1.2             2026-07-08  progressive TUI
v1.3             2026-07-15  polish + docs
v1.4             2026-07-22  release
```
