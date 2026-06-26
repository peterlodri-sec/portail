---
name: "🔁 Loop Engineering — Full Implementation"
about: "Track the loopeng crate: primitives, engine, state, _next-prompt"
title: "feat(loopeng): full loop-engineering primitives + engine"
labels: loop-engineering, v3.0
assignees: ""
---

## Goal

Implement all loop-engineering primitives natively in Rust as the `loopeng` crate,
integrated with Portail's loop-state-manager, generating `_next-prompt` for handoff.

## Primitives (The Five Building Blocks + Memory)

- [x] **Memory/State** — durable key-value store with tags, recall, TTL
- [ ] **Schedule/Automation** — cron-based loop triggers (cadence_secs, max_iterations)
- [ ] **Worktree** — isolated parallel execution, git worktree management
- [ ] **Skill** — reusable instruction templates, versioned, taggable
- [ ] **Plugin/Connector** — MCP server bindings for external tools
- [ ] **Sub-agent** — maker/checker/researcher delegation with role-based dispatch

## Engine

- [x] LoopEngine struct with plan → execute → evaluate → decide pipeline
- [ ] Real async execution (currently returns stubs)
- [ ] Token budget tracking and enforcement
- [ ] Escalation policy (auto-escalate after N failures)
- [ ] Council decisions (SHIP / ITERATE / ESCALATE) with retry
- [ ] Circuit breaker: stop loop after X consecutive failures

## Integration

- [ ] Wire loopeng into AppState (alongside loop-state-manager)
- [ ] `portail loop run <schedule>` — trigger a loop iteration
- [ ] `portail loop prompt` — generate `_next-prompt.md` for handoff
- [ ] `portail loop council <run-id> <ship|iterate|escalate>` — manual council
- [ ] Expose loop state as MCP tools (discoverable by maki)

## _next-prompt

- [x] NextPrompt struct + markdown formatter
- [x] `_next-prompt.md` file writer
- [ ] Auto-generate at end of every loop iteration
- [ ] Fresh agent reads `_next-prompt` → continues session
- [ ] Session handoff via ACP/maki integration

## Testing

- [x] Memory recall/store tests
- [x] Council decision display tests
- [x] NextPrompt markdown format tests
- [x] SharedLoopEngine thread-safety tests
- [ ] Integration test: run loop → check artifacts
- [ ] Integration test: escalate after N failures
