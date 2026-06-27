# OpenCode Multiplexer Integration — Portail

**Date:** 2026-06-27
**Status:** Approved design — ready for implementation plan
**Scope:** Nix-only. Adds zellij-driven subagent pane support to the Portail dev environment, with a nushell launcher module that mirrors the existing `portail.nu` style.

## Goal

Reproducibly wire the `oh-my-opencode-slim` multiplexer-integration pattern (tmux/zellij panes for subagents) into the Portail repository, so that any developer with the flake checked out can launch opencode inside a zellij session with `multiplexer` config baked in, by running one command.

## Non-Goals

- No new CI workflows.
- No port/replacement of the user-level bash launchers in `~/.config/opencode/`.
- No changes to `devShells.default` or `devShells.light`.
- No tmux support (zellij-only; tmux may be added later behind `type: "auto"`).
- No new public REST/gRPC endpoints on the `portail` binary.

## Decisions (locked by brainstorming)

| Question | Decision |
|---|---|
| Integration boundary | Nix-only (flake + nushell) |
| Multiplexer | zellij |
| Layout | `main-vertical` |
| Pane tab mode | `agent-tab` (dedicated `opencode-agents` tab) |
| Nushell port | New repo-level `nushell/ohmy-slim.nu`; bash launchers untouched |

## Architecture

Three layers, all nix-reproducible:

1. **Flake** (`flake.nix` + new `nix/opencode-mux.nix` + `nix/opencode-mux/default.json`) — adds a `devShells.opencode-mux` shell and `apps.opencode-mux` + `apps.ohmy-mux` one-shots. The JSON template is the single source of truth for the `multiplexer` block.
2. **Nushell** (`nushell/ohmy-slim.nu`) — new module with `ohmy-slim launch`, `ohmy-slim ultra`, `ohmy-slim mux-launch`. Pure-nu; uses `^opencode` not bash.
3. **Generated user config** (`~/.config/opencode/oh-my-opencode-slim.json`) — written idempotently by the nushell launcher via `ohmy-slim write-config`. Behavior:
   - **Absent**: copy the template in verbatim.
   - **Present, valid JSON**: deep-merge the `multiplexer` block only; preserve all other top-level keys byte-for-byte.
   - **Present, parse-fail**: back up to `oh-my-opencode-slim.json.bak.<unix-ts>` and rewrite from template (with a warning).

The existing `devShells.nushell` (`portail-nushell-ops`) keeps its current shape; a single `commands` entry is added so `nix develop .#nushell` exposes `mux-launch`.

## Components

| Component | Path | Purpose |
|---|---|---|
| Flake module | `nix/opencode-mux.nix` | Helper: provides the JSON template path, a `writeConfig` builder, and reusable attrs (`shell`, `app-no-mux`, `app-mux`). |
| JSON template | `nix/opencode-mux/default.json` | The canonical `multiplexer` block + placeholder for other top-level keys (commented). |
| Generated user config | `~/.config/opencode/oh-my-opencode-slim.json` | What oh-my-opencode-slim actually reads. |
| Nushell module | `nushell/ohmy-slim.nu` | Public surface: `ohmy-slim launch`, `ohmy-slim ultra`, `ohmy-slim mux-launch`. |
| Dev shell | `devShells.opencode-mux` | nushell + zellij + opencode (via `llm-agents`). No rust toolchain. Fast to enter. |
| App | `apps.opencode-mux` | `nix run .#opencode-mux` — opencode inside a fresh zellij session, no mux block written. |
| App | `apps.ohmy-mux` | `nix run .#ohmy-mux` — writes config + runs `ohmy-slim mux-launch`. |
| Docs | `docs/contributors/OPENCODE_MUX.md` | Usage + troubleshooting. |
| AGENTS.md link | (existing file) | Add the new doc to the Quick Links table. |

## Data flow

```
$ nix run .#ohmy-mux
   │
   ├─► nix shell hooks PATH (nushell + zellij + opencode on PATH)
   ├─► resolve-free-port              # 49152..65535
   ├─► nushell: ohmy-slim write-config (idempotent merge of multiplexer block)
   ├─► exec: zellij --new-session-with-layout default \
   │       $shellHook \
   │       "opencode --port $port"
   │
   └─► user runs a delegated task in opencode TUI
         └─► oh-my-opencode-slim reads oh-my-opencode-slim.json
               └─► sees multiplexer.type = "zellij"
                     └─► opens new pane in `opencode-agents` tab (agent-tab mode)
```

## Multiplexer JSON (canonical block)

```json
{
  "multiplexer": {
    "type": "zellij",
    "layout": "main-vertical",
    "zellij_pane_mode": "agent-tab"
  }
}
```

Rationale per the doc: `main_pane_size` is tmux-only and is intentionally omitted. `agent-tab` is the doc's default and keeps the user's main session uncluttered.

## Nushell API

```nu
# nushell/ohmy-slim.nu — public surface

# Launch opencode with all standard optimizer env vars.
export def "ohmy-slim launch" [
    --port: int = 4096
    --extra: list<string> = []
] { ... }

# ULTRA launcher: mimalloc + vmtouch preload + QoS hints.
export def "ohmy-slim ultra" [
    --port: int = 4096
    --no-preload
] { ... }

# New: write multiplexer config + start zellij session.
export def "ohmy-slim mux-launch" [
    --port: int            # 0 = auto-pick
    --layout: string = "main-vertical"
] { ... }

# Helper: idempotently merge the multiplexer block into the user config.
export def "ohmy-slim write-config" [
    --path: string = "~/.config/opencode/oh-my-opencode-slim.json"
] { ... }

# Helper: pick a free port in 49152..65535.
export def "ohmy-slim pick-port" [] { ... }
```

All commands follow the existing `portail.nu` convention (`def "namespace verb" [...] { ... }`).

## Flake additions (sketch)

```nix
# In flake.nix, perSystem block:
let
  opencodeMux = import ./nix/opencode-mux.nix {
    inherit pkgs inputs';
    inherit (self'.packages) zellij;
    template = ./nix/opencode-mux/default.json;
  };
in
{
  apps = self'.apps // {
    ohmy-mux = opencodeMux.appMux;
    opencode-mux = opencodeMux.appNoMux;
  };

  devShells.opencode-mux = opencodeMux.shell;
}
```

`nix/opencode-mux.nix` returns `{ shell, appMux, appNoMux }`. Apps are `writeShellScriptBin` wrappers that exec the nushell module — keeps the flake readable.

## Error handling

| Condition | Behavior |
|---|---|
| zellij not on PATH | `mux-launch` prints install hint and falls back to `launch` (background subagents only, no panes). Exit 0. |
| Port collision (chosen port in use) | Retry with new port up to 3×. After 3 failures: print `OPENCODE_PORT=$tried` and exit 1. |
| Config dir missing | Create `~/.config/opencode/` if absent. Honor `$XDG_CONFIG_HOME`. |
| Already inside `$TMUX` or `$ZELLIJ` | Print "refusing to nest" hint; `exec` the inner opencode command in place. Exit 0. |
| Config parse fail on merge | Back up the existing file to `oh-my-opencode-slim.json.bak.<unix-ts>` and rewrite from template. |
| `nu` version < 0.95 in PATH | Error with required version (0.95+ for `merge` + `compact` parity with the rest of the repo). |

## Testing

- `nix flake check` (existing) — must still pass; no new warnings.
- `nix build .#opencode-mux` succeeds on `aarch64-darwin` and `x86_64-linux` (the two CI matrices; skip the others for speed).
- `nu -c "source nushell/ohmy-slim.nu; ohmy-slim pick-port" | str length` returns 5 (port in expected range).
- `nu -c "source nushell/ohmy-slim.nu; ohmy-slim write-config --path /tmp/test.json; open /tmp/test.json | get multiplexer.type"` returns `"zellij"`.
- Round-trip: run `ohmy-slim write-config` twice, assert the file is byte-identical after the second call.
- No new GitHub Actions workflow (Nix-only decision).

## Migration / backwards compatibility

- `devShells.nushell` keeps all its existing packages, env, commands, and shellHook. Only a single `commands` entry named `mux-launch` is added.
- `devShells.default` and `devShells.light` unchanged.
- User-level `~/.config/opencode/ohmy-slim-launch.sh` and `ohmy-slim-ultra.sh` continue to work. The nushell module becomes the documented recommendation; bash scripts are noted as legacy in `OPENCODE_MUX.md`.
- The user's existing `~/.config/opencode/oh-my-opencode-slim.json` is **never overwritten** by anything other than an explicit `ohmy-slim write-config` call, which preserves all top-level keys outside the `multiplexer` block.

## File-by-file change list

| File | Change |
|---|---|
| `flake.nix` | + `apps.opencode-mux`, `apps.ohmy-mux`, `devShells.opencode-mux`; + 1 `commands` entry in `devShells.nushell`. |
| `nix/opencode-mux.nix` | NEW. Helper module returning `{ shell, appMux, appNoMux }`. |
| `nix/opencode-mux/default.json` | NEW. JSON template. |
| `nushell/ohmy-slim.nu` | NEW. Nushell launcher module. |
| `docs/contributors/OPENCODE_MUX.md` | NEW. Usage + troubleshooting. |
| `AGENTS.md` | Add row to Quick Links pointing to `OPENCODE_MUX.md`. |

Six files: 4 new, 2 modified.

## Risks

| Risk | Mitigation |
|---|---|
| oh-my-opencode-slim upstream changes the `multiplexer` block schema | Template is a separate file; bump + commit in one PR. |
| zellij version drift breaks pane-direction mapping | Pin to `pkgs.zellij` from `nixpkgs`; the dev shell uses the locked version. |
| nushell version drift breaks `ohmy-slim.nu` | Pin `nushell` from `nixpkgs`; same version as the rest of the repo. |
| User's existing `oh-my-opencode-slim.json` has a conflicting `multiplexer` block | `write-config` logs a diff and asks for `--force`. |
| `--port` workaround in the doc is removed (upstream `opencode#9099` resolved) | The flake can drop `--port` from the app wrappers; the nushell API stays the same. |

## Open questions (resolved)

- **`ohmy-slim ultra` binary source** → use the nushell module as the source of truth. `ohmy-slim ultra` is implemented in pure nu (mimalloc preload via `DYLD_INSERT_LIBRARIES`, vmtouch-style `dd` page-warm, `nice` QoS, Bun cache env, `OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS`). The user-level `ohmy-slim-ultra.sh` is kept for non-flake users but is no longer the source of truth — `OPENCODE_MUX.md` documents the nushell command as the recommended path and marks the bash script legacy.
- **tmux fallback in `devShells.opencode-mux`** → no. zellij-only. A future spec can add `type: "auto"` if needed.
