# Module Map

> Every module in Portail: purpose, inputs, outputs, and dependencies.
> Updated for v2.1.

---

## Source Modules (`src/`)

### `main.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Binary entry point. CLI dispatch, allocator selection, server startup, shutdown orchestration |
| **Inputs** | CLI args (clap), config file path |
| **Outputs** | Running server process, exit code |
| **Dependencies** | All modules (orchestrates everything) |
| **Key types** | `Cli`, `Commands` |

### `lib.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Crate root. Module declarations, `AppState` struct, re-exports |
| **Inputs** | Config, subsystem handles |
| **Outputs** | `AppState` (shared state container) |
| **Dependencies** | All modules (declares them) |
| **Key types** | `AppState`, `Config` |

### `proxy.rs`
| Field | Value |
|-------|-------|
| **Purpose** | HTTP router. 60+ endpoints registered on axum `Router`. Middleware stack (auth, rate-limit, request-id, tracing) |
| **Inputs** | `AppState`, HTTP requests |
| **Outputs** | `axum::Router`, HTTP responses |
| **Dependencies** | auth, rate_limit, sessions, telemetry, gateway, cdn, events, hooks, mcp, a2a, a2c, dns, graphql, sentinel, plugins |
| **Key types** | `Router`, `MethodRouter`, `Middleware` |

### `config.rs`
| Field | Value |
|-------|-------|
| **Purpose** | TOML config loader. Default values, validation, CLI overrides |
| **Inputs** | File path (CLI arg), `portail.toml` |
| **Outputs** | `Config` struct |
| **Dependencies** | serde, toml |
| **Key types** | `Config`, `ListenConfig`, `CdnConfig`, `AuthConfig` |

### `types.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Hot-path types. `BoundedMeta` (max 16 entries, key≤128B, val≤512B), `MetaMap` |
| **Inputs** | Raw key-value pairs |
| **Outputs** | Validated, bounded metadata containers |
| **Dependencies** | None (standalone) |
| **Key types** | `BoundedMeta`, `MetaKey`, `MetaValue` |

### `auth.rs`
| Field | Value |
|-------|-------|
| **Purpose** | JWT (RS256/ES256) + static API-key authentication. Bypass paths |
| **Inputs** | Request headers (`Authorization`), config |
| **Outputs** | Auth decision (pass/deny), principal identity |
| **Dependencies** | config, jsonwebtoken |
| **Key types** | `AuthLayer`, `AuthError`, `Claims` |

### `rate_limit.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Token bucket rate limiting (governor). Per-key, per-endpoint, per-IP |
| **Inputs** | Request metadata (key, endpoint, IP) |
| **Outputs** | Rate-limit decision (pass/block), headers (`X-RateLimit-*`) |
| **Dependencies** | governor, config |
| **Key types** | `RateLimitLayer`, `RateLimitState`, `BucketConfig` |

### `sessions.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Per-session analytics. Tracks request count, duration, tokens |
| **Inputs** | Request/response pairs |
| **Outputs** | Session metrics, analytics events |
| **Dependencies** | events |
| **Key types** | `SessionManager`, `SessionData` |

### `telemetry.rs`
| Field | Value |
|-------|-------|
| **Purpose** | OTLP trace export (gRPC), JSON structured logging (tracing-appender), Prometheus metrics |
| **Inputs** | Spans, events, metrics from all modules |
| **Outputs** | OTLP gRPC stream, log files, `/metrics` endpoint |
| **Dependencies** | tracing, opentelemetry, metrics-exporter-prometheus |
| **Key types** | `TelemetryGuard`, `LogDir` |

### `shutdown.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Panic hooks (production backtrace capture), graceful shutdown (signal handlers) |
| **Inputs** | OS signals, panic events |
| **Outputs** | Clean process termination |
| **Dependencies** | tokio signal, tracing |
| **Key types** | — |

### `supervisor.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Background task supervisor. Monitors spawned tasks, restarts on failure, tracks lifecycle |
| **Inputs** | Task futures, restart policy |
| **Outputs** | Running task handles, health status |
| **Dependencies** | tokio, tracing |
| **Key types** | `Supervisor`, `TaskHandle` |

### `nats_bridge.rs`
| Field | Value |
|-------|-------|
| **Purpose** | NATS event bus bridge. Event publish/subscribe across portail instances |
| **Inputs** | Local events (EventLog), NATS messages |
| **Outputs** | NATS publish, NATS subscription events |
| **Dependencies** | async-nats, events |
| **Key types** | `NatsBridge`, `NatsConfig` |

### `config_watcher.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Self-healing config file watcher. File-system notification, validation, hot-reload, rollback |
| **Inputs** | Config file path, filesystem events (inotify/FSEvents) |
| **Outputs** | Updated `Config`, rollback on validation failure |
| **Dependencies** | notify, config |
| **Key types** | `ConfigWatcher`, `WatchEvent` |

### `file_cache.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Content-addressable file cache. blake3 hashing, mmap zero-copy reads, LRU eviction |
| **Inputs** | File bytes, cache key (blake3 hash) |
| **Outputs** | Cached file handle, mmap region |
| **Dependencies** | blake3, memmap2 |
| **Key types** | `FileCache`, `CacheEntry` |

### `store.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Event store. SQLite (rusqlite, WAL mode) primary, Turso/libSQL opt-in backend |
| **Inputs** | Events to persist, query parameters |
| **Outputs** | Stored events, query results |
| **Dependencies** | rusqlite, libsql |
| **Key types** | `StoreBackend`, `StoreConfig`, `EventRow` |

### `graphql.rs`
| Field | Value |
|-------|-------|
| **Purpose** | async-graphql schema. Query events, publish mutations, subscription for live events |
| **Inputs** | GraphQL queries/mutations/subscriptions |
| **Outputs** | GraphQL responses, event streams |
| **Dependencies** | async-graphql, events, store |
| **Key types** | `Schema`, `QueryRoot`, `MutationRoot`, `SubscriptionRoot` |

### `drift.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Drift detection. Capture request/response pairs, replay them, diff outputs |
| **Inputs** | HTTP request/response captures |
| **Outputs** | Drift report (diff between captured and current behavior) |
| **Dependencies** | reqwest, serde_json |
| **Key types** | `DriftDetector`, `CaptureSet` |

### `spec_verify.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Route spec verification. Compare `spec.routes.toml` with actual router |
| **Inputs** | spec.routes.toml, running server |
| **Outputs** | Spec compliance report |
| **Dependencies** | toml, serde, proxy |
| **Key types** | `SpecVerifier` |

### `fuzz_route.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Route fuzzer. Sends random/malformed requests to every endpoint |
| **Inputs** | Server URL, route list |
| **Outputs** | Crash report (panic status per route) |
| **Dependencies** | reqwest, rand |
| **Key types** | `FuzzRunner` |

### `target_router.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Multi-backend request routing. Routes to different upstreams based on rules |
| **Inputs** | Request, routing config |
| **Outputs** | Target backend URL |
| **Dependencies** | config |
| **Key types** | `TargetRouter`, `RouteRule` |

### `upgrader.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Connection upgrader. WebSocket, HTTP/2, protocol negotiation |
| **Inputs** | HTTP connection streams |
| **Outputs** | Upgraded protocol connections |
| **Dependencies** | tokio-tungstenite, hyper |
| **Key types** | `Upgrader`, `UpgradeError` |

### `plugin_hooks.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Plugin lifecycle hooks. Init, request, response, shutdown callbacks |
| **Inputs** | Plugin events from Vaked registry |
| **Outputs** | Hook execution, plugin state changes |
| **Dependencies** | portail-vaked, portail-plugin-sdk |
| **Key types** | `PluginHookManager`, `HookPoint` |

### `release_audit.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Release audit trail. Records build info, dependency tree, license checks |
| **Inputs** | Build metadata, dependency info |
| **Outputs** | Audit report, provenance data |
| **Dependencies** | cargo_metadata |
| **Key types** | `ReleaseAudit` |

### `test_utils.rs`
| Field | Value |
|-------|-------|
| **Purpose** | Shared test utilities. `test_config()`, `test_app_state()`, random port binding |
| **Inputs** | Config overrides |
| **Outputs** | Pre-configured test state |
| **Dependencies** | config, proxy |
| **Key types** | `TestContext` |

---

## Feature Modules (`src/<module>/`)

### `a2a/` — Agent-to-Agent Protocol
| Field | Value |
|-------|-------|
| **Purpose** | Google Agent-to-Agent protocol: agent cards, task lifecycle, WebSocket streaming |
| **Inputs** | A2A HTTP/WS requests |
| **Outputs** | Agent card, task status, streaming updates |
| **Dependencies** | serde, tokio-tungstenite, reqwest |
| **Key types** | `AgentCard`, `TaskLifecycle`, `A2AState` |

### `a2c/` — Agent-to-Consumer Chat
| Field | Value |
|-------|-------|
| **Purpose** | Human-facing chat API with tool use, streaming, token tracking. Commands: `/research`, `/code`, `/review`, `/register` |
| **Inputs** | Chat messages, tool commands |
| **Outputs** | Streaming LLM responses, tool call results |
| **Dependencies** | gateway, hooks, sessions |
| **Key types** | `ChatSession`, `ToolCall`, `A2CState` |

### `cdn/` — CDN Cache
| Field | Value |
|-------|-------|
| **Purpose** | Two-tier cache (Moka in-memory + blake3 disk) with mmap zero-copy reads |
| **Inputs** | Content requests, cache keys |
| **Outputs** | Cached content (memory/disk), cache stats |
| **Dependencies** | moka, blake3, memmap2, tokio |
| **Key types** | `CacheManager`, `CacheEntry`, `CacheStats` |

### `ci/` — CI Status
| Field | Value |
|-------|-------|
| **Purpose** | CI status webhook endpoint. Receives GitHub webhook events |
| **Inputs** | GitHub webhook payloads |
| **Outputs** | CI status events, badge endpoint |
| **Dependencies** | events, serde |
| **Key types** | `CiStatus`, `Badge` |

### `cli/` — CLI & TUI Dashboard
| Field | Value |
|-------|-------|
| **Purpose** | Interactive TUI dashboard (ratatui) + non-interactive CLI commands. Subcommands: `status`, `events`, `hooks`, `cache`, `health`, `config`, `init`, `serve`, `completions`, `pkg-ctx`, `loop`, `guide`, `learn`, `install`, `setup`, `complexity` |
| **Inputs** | CLI args, server state (HTTP) |
| **Outputs** | TUI rendering, CLI output |
| **Dependencies** | ratatui, clap, all server modules (read-only via HTTP) |
| **Key types** | `Dashboard`, `Cli`, `Commands` |

### `discovery/` — Service Discovery
| Field | Value |
|-------|-------|
| **Purpose** | Dynamic backend discovery. Static list, DNS SRV, Consul |
| **Inputs** | Discovery config |
| **Outputs** | Active backend list |
| **Dependencies** | trust-dns-resolver, config |
| **Key types** | `Discovery`, `BackendNode` |

### `dns/` — DNS Resolution
| Field | Value |
|-------|-------|
| **Purpose** | DNS: DoH resolution, network isolation, TTL cache, fallback chain |
| **Inputs** | DNS queries |
| **Outputs** | DNS responses (resolved IPs) |
| **Dependencies** | hickory-resolver, trust-dns-proto |
| **Key types** | `DnsStore`, `DnsResolver`, `DohClient` |

### `events/` — Event System
| Field | Value |
|-------|-------|
| **Purpose** | Ring buffer + broadcast channel + SSE endpoint. Event types: auth, rate-limit, cache, agent, system |
| **Inputs** | Events from any module |
| **Outputs** | SSE stream, event history |
| **Dependencies** | tokio broadcast |
| **Key types** | `EventLog`, `Event`, `SseStream` |

### `gateway/` — AI Gateway
| Field | Value |
|-------|-------|
| **Purpose** | AI upstream forwarding. Stream proxy to LiteLLM, OpenAI, Anthropic, Ollama. Token counting, cost tracking |
| **Inputs** | LLM chat completion requests |
| **Outputs** | Streamed LLM responses, token usage |
| **Dependencies** | reqwest, serde, hooks |
| **Key types** | `GatewayForwarder`, `UpstreamConfig`, `TokenUsage` |

### `godfather/` — System Monitor
| Field | Value |
|-------|-------|
| **Purpose** | System resource monitor + webhook alerts. CPU, memory, disk, network, process tracking |
| **Inputs** | System metrics (sysinfo) |
| **Outputs** | Alert webhooks, health events |
| **Dependencies** | sysinfo, reqwest, events |
| **Key types** | `Godfather`, `AlertRule`, `SystemState` |

### `hooks/` — Prompt Injection
| Field | Value |
|-------|-------|
| **Purpose** | Per-message/per-event prompt injection. CRUD API for hook rules (match → inject system message) |
| **Inputs** | Hook rules (CRUD), request context |
| **Outputs** | Transformed request with injected prompts |
| **Dependencies** | serde, config |
| **Key types** | `HookStore`, `HookRule`, `InjectionPoint` |

### `mcp/` — MCP Gateway
| Field | Value |
|-------|-------|
| **Purpose** | MCP sidecar proxy. Unix socket launcher, tool discovery, execution relay |
| **Inputs** | MCP tool requests |
| **Outputs** | MCP tool responses, sidecar process |
| **Dependencies** | tokio, serde_json, reqwest |
| **Key types** | `McpProxy`, `SidecarConfig`, `McpTool` |

### `orchestrator/` — Loop Orchestration
| Field | Value |
|-------|-------|
| **Purpose** | Loop engine orchestration. Fleet orchestrator, AgentTool trait, ToolRegistry, FanOutEngine |
| **Inputs** | Loop tasks, tool definitions |
| **Outputs** | Orchestrated tool execution, plan results |
| **Dependencies** | loopeng, pkg-ctx |
| **Key types** | `FleetOrchestrator`, `ToolRegistry`, `AgentTool` |

### `plugins/` — Built-in Plugins
| Field | Value |
|-------|-------|
| **Purpose** | Built-in plugins: tinyurl (URL shortener), tracer (request tracing), redis_cache |
| **Inputs** | Plugin-specific requests |
| **Outputs** | Plugin responses |
| **Dependencies** | config, redis |
| **Key types** | `Plugin`, `PluginConfig` |

### `sentinel/` — Health Watchdog
| Field | Value |
|-------|-------|
| **Purpose** | Background health watchdog. 30s tick → CDN scrub stats → health check → publish to EventLog |
| **Inputs** | System health checks |
| **Outputs** | Health events, status reports |
| **Dependencies** | events, cdn, reqwest |
| **Key types** | `Sentinel`, `HealthStatus` |

---

## Workspace Crates

### `loopeng` — Loop Engine
| Field | Value |
|-------|-------|
| **Purpose** | 5 building blocks (Schedule, Worktree, Skill, Plugin, Sub-agent) + Memory/State + plan→execute→evaluate→decide pipeline |
| **Key types** | `LoopEngine`, `Schedule`, `Worktree`, `TokenBudget`, `CircuitBreaker` |
| **Tests** | 19 |

### `loop-state-manager` — HITL State
| Field | Value |
|-------|-------|
| **Purpose** | Human-in-the-loop state management. Tasks, phases, decisions (Ship / Iterate / Escalate) |
| **Key types** | `LoopStateManager`, `Phase`, `Decision` |
| **Tests** | 6 |

### `pkg-ctx` — Docs MCP Server
| Field | Value |
|-------|-------|
| **Purpose** | Local-first documentation MCP server. Git clone, FTS5 index, semantic search, in-memory cache |
| **Key types** | `PkgCtx`, `FtsIndex`, `DocCache` |
| **Tests** | 18 |

### `portail-agents` — CI Agents
| Field | Value |
|-------|-------|
| **Purpose** | CI agents: drift-detect, spec-verify, fuzz-route, chore-bot, nullclaw (network-native heartbeat) |
| **Key types** | `NullClaw`, `DriftDetect`, `SpecVerify`, `FuzzRoute`, `ChoreBot` |
| **Tests** | — |

### `portail-plugin-sdk` — Plugin SDK
| Field | Value |
|-------|-------|
| **Purpose** | `.vaked` plugin SDK. Trait definitions, lifecycle hooks, ABI |
| **Key types** | `VakedPlugin`, `PluginManifest` |
| **Tests** | — |

### `portail-vaked` — Plugin Registry
| Field | Value |
|-------|-------|
| **Purpose** | Plugin registry, CLI tooling, Nix lowering for plugin packages |
| **Key types** | `PluginRegistry`, `VakedCli` |
| **Tests** | — |

---

## External Crates & Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| axum | 0.8 | HTTP framework |
| tokio | 1 | Async runtime |
| clap | 4 | CLI argument parsing |
| serde | 1 | Serialization |
| reqwest | 0.12 | HTTP client |
| moka | 0.12 | In-memory cache |
| blake3 | 1 | Hashing (SIMD) |
| async-graphql | 7 | GraphQL |
| rusqlite | 0.32 | SQLite |
| governor | 0.6 | Rate limiting |
| ratatui | 0.29 | TUI rendering |
| jsonwebtoken | 9 | JWT auth |
| tracing | 0.1 | Structured logging |
| opentelemetry | 0.27 | Distributed tracing |
| async-nats | 0.38 | NATS messaging |

---

## Module Dependency Graph

```
                          main.rs
                             │
                         lib.rs (AppState)
                        ┌──────┼──────┐
                        │      │      │
                    proxy.rs   cli/   config.rs
                   ┌───┼───┐     │
                   │   │   │     │
              auth.rs  │  gateway/   ... (handlers)
          rate_limit.rs │
        sessions.rs     │
      telemetry.rs     │
                   event_log (events/)
                        │
         ┌──────────────┼──────────────┐
         │              │              │
      sentinel/      godfather/    nats_bridge.rs
      cdn/           hooks/        store.rs
      mcp/           a2a/          graphql.rs
      dns/           a2c/          …
```

### Dependency Rules

- **Downward**: config → all (Config is read-only after init)
- **Sideways**: modules depend on events/ (EventLog) for inter-module communication
- **Upward**: nothing depends on main.rs or proxy.rs (they orchestrate)
- **External**: workspace crates (loopeng, pkg-ctx, etc.) depend only on SDK crates
