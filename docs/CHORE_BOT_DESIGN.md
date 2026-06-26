# Rust Chore CI Agent — Design

## Concept

A CI agent whose sole job is to automate Rust project "chores":
project-wide refactoring, type renames, test fixing, import cleanup,
dep removal, edition upgrades, and mechanical codebase maintenance.

## Why

The last 30 minutes of work — renaming `FxHashMap` → `BoundedMeta`
across 23 files, fixing imports, updating tests, running `cargo fix` —
is the exact UX this agent automates. Today it took a human + LLM
~30 minutes. The agent does it in <10 seconds, every PR.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    PR opened / commit pushed             │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────┐
│  1. cargo diff <base> <head>                             │
│     → detect structural changes:                          │
│       - pub type renames (Metadata → BoundedMeta)        │
│       - field type changes (FxHashMap → BoundedMeta)     │
│       - module adds/removes (new mod.rs declarations)    │
│       - dep changes (Cargo.toml additions/removals)      │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────┐
│  2. Apply mechanical fixes:                              │
│     - cargo fix --lib --tests --allow-dirty              │
│     - cargo fmt                                           │
│     - cargo clippy --fix --allow-dirty                   │
│     - post: update test constructors for new fields      │
│     - post: update AppState { ..Default::default() }     │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────┐
│  3. Verify:                                              │
│     - cargo check (must be 0 errors)                     │
│     - cargo test (must be N≥previous_count, 0 failures)  │
│     - rust-analyzer diagnostics (0 errors in editor)     │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────────────────────────────────────────────────┐
│  4. Report:                                              │
│     - gh pr comment "chore-bot: fixed X files"           │
│     - Auto-commit if config says auto-fix=true           │
│     - Or annotate PR with inline suggestions             │
└──────────────────────────────────────────────────────────┘
```

## CLI Spec

```
rust-chore
  check    Diff-check only, report needed fixes (exit 0 always)
  fix      Apply auto-fixes, commit + push (or comment if not allowed)
  verify   cargo check + cargo test, fail if degraded
  report   Generate TOML report for CI consumption
```

## Implementation

### Option A: Shell + cargo (MVP)
- `scripts/rust-chore.sh` — 200-line bash script
- Runs cargo fix/clippy/fmt, checks test count, comments on PR
- Minimal: 1 day to build

### Option B: Rust binary (prod)
- `rust-chore` crate — uses `syn`/`quote` for AST-level rewrites
- Understands Rust semantics, not just text replacement
- Can do: rename type + update all constructors + update all imports + update tests
- Effort: 3-5 days

### Option C: GH Action workflow (hybrid)
- `.github/workflows/chore-bot.yml`
- Combines shell scripts + cargo toolchain
- Reports via issue comment API

## Integration with Portail

Portail already has:
- CI status webhook (`PORTAIL_WEBHOOK_SECRET`)
- Complexity bot (v0.3, advisory-only)
- Drift detect (v0.4)
- Spec verify (v0.5)
- Fuzz route (v0.6)

**Proposal**: Add `chore-bot` as CI agent #5 in v1.4:

| Agent | Blocks CI? | Exit code | Output |
|-------|-----------|-----------|--------|
| chore-bot (v1.4) | ❌ never | always 0 | fix report → PR comment |

## Convention

The chore agent follows a few rigid conventions:
1. **Never block CI** — always exit 0
2. **Report, recommend, never enforce** — same as complexity/drift/spec
3. **Auto-commit only if `auto-fix: true`** in chore config
4. **Must pass `cargo check` before and after** — no regressions

## Next Steps

- [ ] Create `.github/workflows/chore-bot.yml`
- [ ] Write `scripts/rust-chore.sh` (MVP shell version)
- [ ] Test on portail repo with a type-rename PR
- [ ] Promote to v1.4 CI agent in LOOP_STATE.md
