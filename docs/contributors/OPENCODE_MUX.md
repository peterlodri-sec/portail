# OpenCode Multiplexer Integration

Launch `oh-my-opencode-slim` inside a zellij session so that delegated subagents open live panes you can watch. This is the Portail-flavored version of the [upstream multiplexer-integration guide](https://github.com/alvinunreal/oh-my-opencode-slim/blob/master/docs/multiplexer-integration.md).

## Quick start

```bash
# Enter the mux shell (nushell + zellij + opencode on PATH, no rust toolchain).
nix develop .#opencode-mux

# Or run the one-shot app (writes config + starts zellij + launches opencode).
nix run .#ohmy-mux

# Or from the default devshell — the nushell module is on PATH once nushell is loaded:
nu -c "use ../nushell/ohmy-slim.nu *; ohmy-slim mux-launch --port 0"
```

The first time you run `mux-launch` (or `ohmy-slim write-config`), this JSON is merged into `~/.config/opencode/oh-my-opencode-slim.json`:

```json
{
  "multiplexer": {
    "type": "zellij",
    "layout": "main-vertical",
    "zellij_pane_mode": "agent-tab"
  }
}
```

If the file already exists, only the `multiplexer` block is touched. Every other top-level key is preserved byte-for-byte.

## What you get

- Your main opencode session on the left.
- A dedicated `opencode-agents` zellij tab where every delegated subagent opens a fresh pane.
- zellij's `Ctrl+p` + arrow keys to switch panes, `Ctrl+t` + `n` to jump to the agents tab.

## Nushell commands

| Command | Purpose |
|---|---|
| `ohmy-slim launch --port 4096` | Opencode with the standard env (Bun cache, Node heap tuning, background subagents). |
| `ohmy-slim ultra --port 4096` | Pure-nu port of the legacy `ohmy-slim-ultra.sh`: mimalloc preload, config page-warm, `nice -n -10`. |
| `ohmy-slim mux-launch --port 0` | Write config + start zellij session + run opencode. `--port 0` picks a random high port. |
| `ohmy-slim write-config` | Idempotently merge the `multiplexer` block into your user config. |
| `ohmy-slim pick-port` | Pick a random port in 49152..65535. |

All commands follow the `def "namespace verb"` style used by `nushell/portail.nu`.

## Legacy bash launchers

The user-level scripts `~/.config/opencode/ohmy-slim-launch.sh` and `~/.config/opencode/ohmy-slim-ultra.sh` continue to work. They are now documented as **legacy**: the nushell module is the source of truth for new development. We keep the bash scripts to avoid breaking anyone who symlinks them or runs them outside a Nix shell.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `mux-launch` falls back to `launch` | `zellij` not on PATH. `nix develop .#opencode-mux` to add it, or install zellij via your package manager. |
| Pane direction is wrong | We use `zellij_pane_mode: "agent-tab"`. Subagents open in a dedicated `opencode-agents` tab. Switch with `Ctrl+t n` (zellij next-tab) or click the tab bar. |
| `port already in use` | `--port 0` to let the launcher pick a free high port. |
| `refusing to nest` | You're already inside tmux or zellij. The launcher won't double-nest; it just runs `ohmy-slim launch` in place. |
| Subagent panes don't appear | Confirm `multiplexer.type` is `zellij` in your user config. The oh-my-opencode-slim plugin reads it on opencode startup — restart opencode after the first `write-config`. |
| `nix run .#ohmy-mux` fails with "no such attribute" | Your llm-agents input may not export `packages.opencode`. Check `nix flake metadata` and the upstream `llm-agents.nix` README. |

## How it works

`oh-my-opencode-slim` reads `~/.config/opencode/oh-my-opencode-slim.json` and, when it sees a `multiplexer` block, opens new panes via `tmux` or `zellij` whenever a subagent session starts. The Portail flake writes a JSON template (`nix/opencode-mux/default.json`) and the nushell module (`nushell/ohmy-slim.nu`) merges it into your user config on demand. See [`docs/superpowers/specs/2026-06-27-opencode-mux-integration-design.md`](../superpowers/specs/2026-06-27-opencode-mux-integration-design.md) for the full design.
