---
name: "Loop Engineering — Full Implementation"
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
- [x] **Schedule/Automation** — schedule struct with cadence_secs, max_iterations, pattern, enabled flag
- [x] **Worktree** — isolated parallel execution context (struct + status enum)
- [x] **Skill** — reusable instruction templates, versioned, taggable
- [x] **Plugin/Connector** — MCP server bindings config (transport, command, args)
- [x] **Sub-agent** — maker/checker/researcher delegation with role, model, instruction, max_turns

## Engine

- [x] LoopEngine struct with plan → execute → evaluate → decide pipeline
- [x] Real async execution — Executor dispatches to sub-agents and skills, estimates token costs
- [x] Token budget tracking and enforcement (rejects when budget exceeded)
- [x] Escalation policy — auto-escalate after N consecutive failures
- [x] Council decisions (SHIP / ITERATE / ESCALATE) with scoring-based logic (pass at >=0.8, iterate >=0.4, escalate below)
- [x] Circuit breaker — stop loop after X consecutive failures, manual reset
- [x] Council override — `engine.override_decision(run_id, decision)` for manual intervention

## Integration

- [x] `portail loop run <schedule>` — trigger N loop iterations
- [x] `portail loop prompt` — generate `_next-prompt.md` for handoff
- [x] `portail loop council <run-id> <ship|iterate|escalate>` — manual council override
- [x] `portail loop reset-circuit` — reset circuit breaker
- [x] `portail loop config` — show engine config and building block counts
- [x] `portail loop schedules` — list registered schedules
- [ ] Wire loopeng into AppState (alongside loop-state-manager) for live server mode
- [ ] Expose loop state as MCP tools (discoverable by maki)
- [ ] GPT-tokenizer token budget enforcement (instead of estimated)

## _next-prompt

- [x] NextPrompt struct + markdown formatter
- [x] `_next-prompt.md` file writer
- [x] Auto-generated at end of each `loop run` command
- [ ] Fresh agent reads `_next-prompt` → continues session
- [ ] Session handoff via ACP/maki integration

## Testing

- [x] Memory recall/store tests
- [x] Council decision display tests
- [x] NextPrompt markdown format tests
- [x] SharedLoopEngine thread-safety tests
- [x] Integration test: run loop → check artifacts (19 tests)
- [x] Integration test: escalate after N failures
- [x] Circuit breaker open/reset tests
- [x] Token budget enforcement test
- [x] Max iterations enforcement test
- [ ] E2E: `portail loop run` CLI integration test
