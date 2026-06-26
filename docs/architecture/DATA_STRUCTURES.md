# Data Structures Analysis — Core Modules & Hot Paths

**Purpose:** Audit every major data structure on allocation, copy, and lifetime
in the hot path. Identify optimization opportunities.

---

## Hot Paths

```
Request arrives → rate_limit.check → auth.verify → gateway.forward → response
                    │                    │            │
                    ▼                    ▼            ▼
              governor::GCRA       jwt::validate   reqwest::send
              (lock-free)          (allocation)     (syscall)
```

### 1. Rate Limiter — `src/rate_limit.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `RateLimiter` | Created once at startup, `Arc`-shared | Never copied |
| `GovernorState` | `governor::gcra::GCRA` inside `moka::Cache` | Clone on insert |
| `RateLimitConfig` | Deserialized by figment, immutable | Copy via Clone |

**Hot path cost:** Zero alloc per check (GCRA is lock-free, no heap).
**Optimization:** ✅ Lock-free hot path.

### 2. Auth — `src/auth.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `AuthState` | Created once, `Option<Arc<AuthState>>` in AppState | Never copied |
| `JwtValidationResult` | Stack-allocated | Let bindings |
| `AuthConfig` | Figment, immutable | Clone on config reload |

**Hot path cost:** JWT parsing allocates once per request.
**Optimization:** ✅ Acceptable (1 alloc per request is negligible).

### 3. Gateway — `src/gateway/mod.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `reqwest::Client` | Created once, connection pool shared | Arc internally |
| `reqwest::Response` | Streamed body, no eager allocation | Consumed once |
| Hop-by-hop header filter | `HashSet<&'static str>` | Never copied |

**Hot path cost:** Connection pool lookup + header clone.
**Optimization:** ✅ Connection pooling via reqwest.

### 4. MCP Proxy — `src/mcp/mod.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| Frame buffer (`BytesMut`) | Pre-allocated with `with_capacity` | Moved into stream |
| Response (`Bytes`) | Allocated per response | Consumed once |
| Header map (`HashMap<String,String>`) | Allocated per request | Cloned from axum headers |

**Hot path cost:** 3 allocations per request (frame buf, response buf, header map).
**Optimization:** ⚠️ Header map allocation is wasteful — use `HeaderMap` directly.

### 5. Config — `src/config.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `Config` | Created once via figment `extract()` | `RwLock<Config>` for reload |
| `TargetConfig` | Vec of structs, figment-parsed | Clone on read |
| `McpServerEntry` | Vec of structs, figment-parsed | Clone on read |

**Hot path cost:** Zero (config is read at startup, not per-request).
**Optimization:** ✅ Mutable only on SIGHUP reload.

### 6. Event Log — `src/events/mod.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `EventLog` | Ring buffer of 2000 slots, pre-allocated | Never copied |
| `AgentEvent` | Boxed into ring slot | Clone on broadcast |
| Broadcast channel | `tokio::sync::broadcast` (2048 cap) | Cloned per subscriber |

**Hot path cost:** 1 alloc per published event + N clones for N subscribers.
**Optimization:** ⚠️ For 1000+ events/sec, consider mmap ring buffer.

### 7. Supervisor — `src/supervisor.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `Supervisor` | Arc-shared, mutable state | `Arc` cloned per reference |
| Task map | `HashMap<Uuid, TaskState>` | Entry per task |
| Event subscription | Broadcast receiver | Clone per event |

**Hot path cost:** Task creation + event publish.
**Optimization:** ✅ Fine for supervision workloads.

### 8. Proxy Router — `src/proxy.rs`

| Type | Allocation Strategy | Copy Semantics |
|------|-------------------|----------------|
| `axum::Router` | Built once, tower service | Arc internally |
| Request body (Bytes) | Streamed, no eager load | Consumed once |
| Response body (Bytes) | Streamed | Consumed once |
| `AppState` | Arc<AppState>, !Copy | Arc clone (cheap) |

**Hot path cost:** Zero alloc per request (axum reuses buffers).
**Optimization:** ✅ SOTA.

---

## Summary

| Module | Hot Path Allocs | Concern Level |
|--------|---------------|---------------|
| rate_limit | 0 | ✅ None |
| auth | 1 per request | ✅ Low |
| gateway | 0 (pooled) | ✅ None |
| mcp proxy | 3 per request | ⚠️ Medium |
| config | 0 at runtime | ✅ None |
| event log | 1 + N clones | ⚠️ Medium |
| supervisor | 1 per event | ✅ Low |
| proxy router | 0 | ✅ None |

## Priority Optimizations

1. **MCP frame header map** (`src/mcp/mod.rs:182-189`): Replace
   `HashMap<String,String>` clone with `HeaderMap` reference pass.
2. **Event log broadcast** (`src/events/mod.rs`): For high-throughput
   deployments, use mmap-based ring buffer instead of tokio broadcast.
3. **Supervisor task map** (`src/supervisor.rs`): Use `dashmap` instead of
   `Mutex<HashMap>` for concurrent task submissions.
