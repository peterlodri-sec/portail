# Loop State — Portail v2.1.0

## Last session: CI rabbit hole → green
- Simplified CI to dead-simple build+test+clippy+fmt on push/PR
- Fixed ~15 clippy warnings (needless borrows, sort_by_key, large enums, struct update)
- All local: 0 warnings, 0 errors, 239 tests pass
- Remote CI: green on first try after simplification

## What's shipped this release
| Area | Status |
|---|---|
| PHILOSOPHY.md | done |
| pkg-ctx crate (FTS5 SQLite docs MCP server) | done |
| loopeng real engine (token budget, circuit breaker, escalation) | done |
| Fleet orchestrator (AgentTool trait, ToolRegistry, FanOutEngine) | done |
| 3-pane TUI dashboard (banner, log, agent matrix) | done |
| A2C commands (/research, /code, /review, /register) | done |
| SOTA Nix flake (flake-parts, devshell, treefmt, git-hooks) | done |
| Shell completions (portail completions bash/zsh/fish) | done |
| deny.toml | done |
| /api-docs/openapi.json | done |
| spawn_blocking for SQLite ops | done |
| CI: green, simple, fast | done |

## Next
1. **pkg-ctx integration**: wire into loopeng schedules for auto re-index
2. **AppState live mode**: verify loop_runner + pkg_ctx_memory work in server mode
3. **E2E tests**: integration tests for orchestrator dispatch + pkg-ctx search
4. **v2.1 release tag**: cut the release

## Commands
```bash
# Build
cargo check
cargo test --workspace
cargo clippy --all-targets

# Run
cargo run -- loop status
cargo run -- pkg-ctx list
cargo run -- completions bash

# CI (local)
cargo check && cargo test && cargo clippy && cargo fmt --check
```
