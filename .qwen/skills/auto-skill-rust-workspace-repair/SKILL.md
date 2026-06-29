---
name: rust-workspace-repair
description: Systematic approach to fixing a broken Rust workspace with missing deps, wrong imports, trait shadowing, and struct field drift across many files.
source: auto-skill
extracted_at: '2026-06-27T12:00:00.000Z'
---

# Rust Workspace Repair

When a Rust workspace fails to compile due to accumulated drift (missing dependencies, renamed types, new struct fields, wrong package names), follow this ordered approach.

## Step 1: Identify the first compilation blocker

Run `cargo check` and grab the **first** error. Do not try to fix everything at once — each fix may reveal or resolve subsequent errors.

```bash
cargo check 2>&1 | grep "^error" | head -5
```

## Step 2: Fix missing dependencies first

If errors mention "cannot find module or crate" or "unresolved import":

- Check if the crate exists in `Cargo.toml` at workspace root or member level
- If it exists in workspace `members` but not in `[dependencies]`, add it
- Watch for package rename syntax: `foo-bar = { version = "1.0", package = "foo_bar" }` (crates.io uses hyphens, Rust code uses underscores)
- Enable required features: `foo = { version = "1.0", features = ["sessions"] }`

## Step 3: Fix trait shadowing

When `use some::module::SomeName` imports a struct that shadows a trait of the same name from `prelude::*`:

```rust
// WRONG — struct shadows trait from prelude
use adk_rust::prelude::*;
use adk_rust::runner::InvocationContext;  // struct, not the trait

// CORRECT — import the trait explicitly
use adk_rust::prelude::*;
use adk_rust::InvocationContext;  // the trait
```

Check the actual crate's `lib.rs` re-exports to find the correct path.

## Step 4: Fix missing struct fields

When `missing field X in initializer` appears:

- Find the struct definition: `grep -rn "pub struct TheStruct" src/`
- Add all missing fields with sensible defaults
- Common patterns for new fields: `Arc::new(...::new())`, `None`, `Default::default()`
- Search for all `AppState {` or similar constructors across test files:
  ```bash
  grep -rn "AppState {" src/ tests/
  ```

## Step 5: Fix renamed types

When "cannot find `OldName`" but "similar name exists: `NewName`":

```bash
sed -i '' 's/OldName/NewName/g' tests/*.rs src/**/*.rs
```

Also fix field access patterns: `task.status.state` → `task.status` when `TaskStatus` changed from struct-with-state to bare enum.

## Step 6: Fix method name changes

When methods were renamed (`.into_dyn()` → `.into()`):

```bash
grep -rn "\.into_dyn()" src/ tests/
```

## Step 7: Verify with full workspace build

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --all-targets
```

## Key commands

| Purpose | Command |
|---------|---------|
| First error only | `cargo check 2>&1 | grep "^error" | head -1` |
| Find all struct constructors | `grep -rn "SomeStruct {" src/ tests/` |
| Find type renames | `grep -rn "OldType\|NewType" src/ tests/` |
| Check workspace members | `grep "members" Cargo.toml` |
| Parallel test run | `cargo test --workspace -- --test-threads=N` |

## Pitfalls to avoid

- **Don't** fix test files before fixing lib — lib errors cascade into test errors
- **Don't** assume `prelude::*` exports everything — check explicit re-exports
- **Don't** ignore warnings — unused imports often indicate wrong type was imported
- **Do** fix one error class at a time: deps → imports → fields → methods → tests
