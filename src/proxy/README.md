# Proxy Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Request Routing                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Client                                                        │
│      │                                                          │
│      ▼                                                          │
│   ┌─────────────────────────────────────────────────────┐       │
│   │                    axum::Router                      │       │
│   ├─────────────────────────────────────────────────────┤       │
│   │  /healthz, /livez, /readyz  → health handlers       │       │
│   │  /v1/chat/*, /v1/messages   → AI gateway            │       │
│   │  /mcp/*                     → MCP sidecar           │       │
│   │  /cdn/*                     → CDN cache             │       │
│   │  /events/*                  → Event log             │       │
│   │  /hooks/*                   → Hook CRUD             │       │
│   │  /a2a/*                     → Agent-to-Agent        │       │
│   │  /a2c/*                     → Agent-to-Consumer     │       │
│   │  /dns/*                     → DNS resolution        │       │
│   │  /metrics                   → Prometheus            │       │
│   │  fallback                   → AI gateway            │       │
│   └─────────────────────────────────────────────────────┘       │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Middleware Stack                                               │
│                                                                 │
│   ┌─────────────────────────────────────────┐                   │
│   │ 1. CORS (permissive)                    │                   │
│   │ 2. TraceLayer (logging)                 │                   │
│   │ 3. metrics_middleware (counters)         │                   │
│   │ 4. request_id_middleware (x-request-id)  │                   │
│   └─────────────────────────────────────────┘                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **build_router()** - One-time setup - O(n) routes
2. **request_id_middleware** - UUID generation - O(1)
3. **metrics_middleware** - Counter increment - O(1)

## Metrics Collected

- `http_requests_total` - Counter by method/path/status
- `http_request_duration_seconds` - Histogram by path
- `health_checks` - Counter
- `hook_injections` - Counter
