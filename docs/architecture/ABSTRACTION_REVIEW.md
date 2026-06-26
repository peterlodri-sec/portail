# Abstraction Review — Hot Paths, Data Flow & Complexity

**Generated:** 2026-07-28
**Scope:** All core modules, crates, and abstractions built during v2→v3

---

## Hot Paths Ranked by Criticality

| Rank | Module | Calls/sec (est.) | Allocs | Type Safety | Complexity |
|------|--------|-----------------|--------|-------------|------------|
| 1 | `proxy.rs::route_to_ai_gateway` | 100-1000 | 0 (streamed) | ✅ Strong | 2/5 |
| 2 | `rate_limit.rs::check_rate` | 100-1000 | 0 (lock-free) | ✅ Strong | 1/5 |
| 3 | `auth.rs::authenticate` | 100-1000 | 1 (JWT parse) | ✅ Strong | 2/5 |
| 4 | `gateway::forward_with_body` | 100-1000 | 0 (pooled) | ✅ Strong | 2/5 |
| 5 | `mcp.rs::proxy_to_sidecar` | 10-100 | 3 (frame, body, headers) | ⚠️ Medium | 3/5 |
| 6 | `plugin_hooks.rs::call_plugin_hooks` | 10-100 | N plugins × 1 ctx | ⚠️ Dynamic | 3/5 |
| 7 | `loop-state-manager::get_state` | 1-10 | 1 clone | ✅ Strong | 1/5 |
| 8 | `target_router.rs::resolve_upstream` | 1-10 | 0 | ✅ Strong | 2/5 |
| 9 | `release_audit.rs::audit_directory` | 0.01 (per release) | N files | ✅ Strong | 2/5 |
| 10 | `pit.rs::scan_proc` | 0.5 (2s interval) | N PIDs | ✅ Strong | 2/5 |

---

## Core Data Flow

```text
Internet ──► axum Router ──► rate_limit ──► auth ──► route_to_ai_gateway
                                     │                 │
                                     │           ┌─────┴──────┐
                                     │           │            │
                                     ▼           ▼            ▼
                                  hook inject   plugin_hooks  target_router
                                  (src hooks)   (.vaked)      (targets[ ])
                                     │           │            │
                                     └───────────┼────────────┘
                                                 │
                                                 ▼
                                          gateway::forward
                                                 │
                                    ┌────────────┴────────────┐
                                    │                         │
                                    ▼                         ▼
                              Remote API              Local mistral.rs
                          (OpenAI/Anthropic)         (candle backend)

Loop State Manager ←──→ DYAD (WS/TUI/CLI) ←──→ Human
Orchestrator ←──→ superpowers skill (kompress)
```

---

## Per-Module Analysis

### 1. proxy.rs — 823 lines — Complexity: 2/5
- **Hot path**: `route_to_ai_gateway()` — reads config (RwLock), resolves target, calls plugin hooks, injects hooks, forwards
- **Data flow**: Config → RwLock read → clone upstream URL → resolved via target_router → consumed by gateway
- **Allocs**: 0 on hot path (body streamed, headers borrowed)
- **Suggestion**: The function is doing too much (target resolution + hooks + forward). Split into middleware chain.
- **Care level**: 2 — straightforward but could be cleaner

### 2. config.rs — 400+ lines — Complexity: 2/5
- **Hot path**: Zero at runtime — config loaded once at startup
- **Data flow**: Figment extract() → Config struct → RwLock<Config> → read on reload
- **Suggestion**: The `targets` vec and `mcp.server_registry` are growing. Consider separate config files per domain.
- **Care level**: 2 — figment handles layering well

### 3. gateway/mod.rs — 180 lines — Complexity: 2/5
- **Hot path**: `forward_with_body()` — builds URL, strips headers, sends via pooled reqwest client
- **Allocs**: 0 on hot path (connection pool reused, headers stripped in place)
- **Suggestion**: Provider-specific path rewriting (target_router::provider_path) is not called here — it's in the caller.
- **Care level**: 2 — clean, focused

### 4. target_router.rs — 130 lines — Complexity: 2/5
- **Hot path**: `resolve_upstream()` — iterates targets, matches on header → model → provider → first
- **Allocs**: 0 (returns reference to existing TargetConfig)
- **Suggestion**: Add caching for the model→target mapping (moka cache keyed by model name)
- **Care level**: 2 — clean

### 5. mcp/mod.rs — 250 lines — Complexity: 3/5
- **Hot path**: `proxy_to_sidecar()` — encodes frame, sends over Unix socket, decodes response
- **Allocs**: 3 per request (frame buffer `BytesMut`, response `Bytes`, header `HashMap`)
- **Suggestion**: The `HashMap<String,String>` for headers is wasteful (see data structures audit). Use `HeaderMap` reference pass.
- **Care level**: 3 — custom binary framing is performance-sensitive and fragile

### 6. plugin_hooks.rs — 120 lines — Complexity: 3/5
- **Hot path**: `call_plugin_hooks()` — locks registry, iterates plugins, filters by hook, calls handler
- **Allocs**: N plugins × 1 HookContext per request
- **Suggestion**: Plugin matching should be cached (hook → plugins map) to avoid iterating all plugins every request.
- **Care level**: 3 — dynamic dispatch through mutex is a potential bottleneck at 1000+ req/s

### 7. loop-state-manager crate — 440 lines — Complexity: 2/5
- **Hot path**: `get_state()` — clones LoopState (Mutex → clone)
- **Allocs**: 1 clone per query (payload is small)
- **Suggestion**: Arc<LoopState> with CAS update instead of Mutex+Clone for the hot read path
- **Care level**: 2 — Mutex contention at 1 query/sec is negligible

### 8. portail-plugin-sdk crate — 300 lines — Complexity: 2/5
- **Hot path**: Zero — SDK is compile-time only (plugin devs use it, not the proxy)
- **Suggestion**: Consider codegen (proc macro) for the `PortailPlugin` trait impl — `#[portail_plugin]` on a module
- **Care level**: 2 — well-structured trait definitions

### 9. portail-vaked crate — 200 lines — Complexity: 2/5
- **Hot path**: Zero — only called at startup or CLI command
- **Suggestion**: `.vaked` files currently only parse metadata — the WASM build/deploy pipeline needs full implementation
- **Care level**: 2 — straightforward

### 10. portail-agents crate — Complexity: 2/5
- **Sub-modules**: nullclaw, CI agents (drift, spec-verify, fuzz-route, chore, research), PIT
- **Hot path**: PIT scans /proc every 2s (Linux only). CI agents run on demand.
- **Suggestion**: The research agent has three search providers but only DuckDuckGo works without API keys. Make the other two optional.
- **Care level**: 2 — well-factored

### 11. release_audit.rs — 825 lines — Complexity: 2/5
- **Hot path**: Zero — runs once per release.
- **Suggestion**: The ELF parser (`check_stripped`) is hand-rolled and fragile. Use the `goblin` crate for real ELF parsing.
- **Care level**: 2 — it works, but the custom ELF parser is technical debt

### 12. upgrader.rs — 260 lines — Complexity: 3/5
- **Hot path**: One per WebSocket upgrade.
- **Allocs**: 1 (frame buffer for handshake read)
- **Suggestion**: The `ws_frame_loop()` runs on a blocking thread. For production, use tokio-tungstenite instead of raw frame handling.
- **Care level**: 3 — the unsafe raw fd extraction is correct but needs careful testing

---

## Complexity Summary

| Module | Lines | Care Level | Priority |
|--------|-------|------------|----------|
| proxy.rs | 823 | 2/5 | 🟢 |
| config.rs | 400+ | 2/5 | 🟢 |
| gateway/mod.rs | 180 | 2/5 | 🟢 |
| target_router.rs | 130 | 2/5 | 🟢 |
| mcp/mod.rs | 250 | 3/5 | 🟡 |
| plugin_hooks.rs | 120 | 3/5 | 🟡 |
| loop-state-manager | 440 | 2/5 | 🟢 |
| portail-plugin-sdk | 300 | 2/5 | 🟢 |
| portail-vaked | 200 | 2/5 | 🟢 |
| portail-agents | multi | 2/5 | 🟢 |
| release_audit.rs | 825 | 2/5 | 🟡(tech debt) |
| upgrader.rs | 260 | 3/5 | 🟡 |

---

## Actionable Improvements

1. **plugin_hooks.rs cache**: Pre-compute hook→plugin mapping at load time instead of scanning all plugins per request.
2. **mcp/mod.rs header map**: Replace `HashMap<String,String>` with `HeaderMap` reference to save 1 alloc.
3. **release_audit ELF parser**: Replace hand-rolled parser with the `goblin` crate.
4. **upgrader ws_frame_loop**: Replace raw WebSocket frame handling with `tokio-tungstenite`.
5. **loop-state-manager**: Swap `Mutex<LoopState>` → `Arc<RwLock<LoopState>>` for concurrent reads.
6. **target_router**: Add moka cache keyed by `(provider_header, model_name)` to skip linear scan.
7. **proxy.rs**: Split `route_to_ai_gateway` into a middleware chain.
