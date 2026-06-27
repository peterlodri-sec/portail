# A2A Module (Agent-to-Agent — Google A2A Protocol v0.2.5)

## Protocol

JSON-RPC 2.0 over HTTP. All methods go through a single endpoint.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/.well-known/agent.json` | Agent Card discovery |
| `POST` | `/a2a` | JSON-RPC 2.0 (all task methods) |
| `POST` | `/a2a/subscribe` | SSE streaming (`tasks/sendSubscribe`) |

### JSON-RPC Methods

| Method | Params | Returns |
|--------|--------|---------|
| `tasks/send` | `{id?, message}` | `Task` |
| `tasks/get` | `{id}` | `Task` |
| `tasks/cancel` | `{id}` | `Task` |
| `tasks/pushNotification/set` | `{id, pushNotificationConfig}` | `Task` |
| `tasks/pushNotification/get` | `{id}` | `PushNotificationConfig` |
| `tasks/sendSubscribe` | `{id?, message}` | SSE stream of `Task` |

### Task States

```
submitted → working → completed
    ↓           ↓
  canceled    failed
              input-required
```

### Example: Send a task

```bash
curl -X POST http://localhost:8787/a2a \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tasks/send",
    "params": {
      "message": {
        "role": "user",
        "parts": [{"type": "text", "text": "Summarize this document"}]
      }
    }
  }'
```

### Example: SSE streaming

```bash
curl -N -X POST http://localhost:8787/a2a/subscribe \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tasks/sendSubscribe",
    "params": {
      "message": {
        "role": "user",
        "parts": [{"type": "text", "text": "Stream this"}]
      }
    }
  }'
```

### Example: Get task

```bash
curl -X POST http://localhost:8787/a2a \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tasks/get","params":{"id":"<task-id>"}}'
```

### Example: Cancel task

```bash
curl -X POST http://localhost:8787/a2a \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tasks/cancel","params":{"id":"<task-id>"}}'
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    A2A Protocol Flow                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Agent Discovery                                               │
│        │                                                        │
│        ▼                                                        │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  GET /.well-known/agent.json                            │   │
│   │  {                                                      │   │
│   │    "name": "portail",                                   │   │
│   │    "capabilities": { "streaming": true },               │   │
│   │    "skills": [{ "id": "proxy", ... }]                   │   │
│   │  }                                                      │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│   JSON-RPC 2.0 (POST /a2a)                                     │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  Submitted │────▶│  Working   │────▶│  Completed │         │
│   └────────────┘     └────────────┘     └────────────┘         │
│        │                   │                   │                 │
│        │                   ▼                   │                 │
│        │           ┌────────────┐              │                 │
│        └──────────▶│  Canceled  │◀─────────────┘                 │
│                    └────────────┘                                │
│                                                                 │
│   SSE Streaming (POST /a2a/subscribe)                           │
│        │                                                        │
│        ▼                                                        │
│   tasks/sendSubscribe → event stream of Task snapshots          │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Task Store (in-memory)                                        │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  HashMap<String, Task>                                  │   │
│   │  - create(id) → Task                                    │   │
│   │  - get(id) → Option<Task>                               │   │
│   │  - update_state(id, state) → Option<Task>               │   │
│   │  - add_message(id, message) → Option<Task>              │   │
│   │  - set_push_config(id, config) → Option<Task>           │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Error Codes

| Code | Meaning |
|------|---------|
| `-32600` | Invalid Request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32603` | Internal error |
| `-32001` | Task not found |
