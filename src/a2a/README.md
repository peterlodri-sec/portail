# A2A Module (Agent-to-Agent)

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
│   │  /.well-known/agent.json                                │   │
│   │  {                                                      │   │
│   │    "name": "portail",                                   │   │
│   │    "capabilities": { "streaming": true },               │   │
│   │    "skills": [{ "id": "proxy", ... }]                   │   │
│   │  }                                                      │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│   Task Lifecycle                                                │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  Submitted │────▶│  Working   │────▶│  Completed │         │
│   └────────────┘     └────────────┘     └────────────┘         │
│        │                   │                   │                 │
│        │                   ▼                   │                 │
│        │           ┌────────────┐              │                 │
│        └──────────▶│  Failed    │◀─────────────┘                 │
│                    └────────────┘                                │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Task Store (in-memory)                                        │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  FxHashMap<String, Task>                                │   │
│   │  - create(id) → Task                                    │   │
│   │  - get(id) → Option<Task>                               │   │
│   │  - update_status(id, status) → Option<Task>             │   │
│   │  - add_message(id, message) → Option<Task>              │   │
│   │  - add_artifact(id, artifact) → Option<Task>            │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **create()** - HashMap insert - O(1)
2. **get()** - HashMap lookup - O(1)
3. **update_status()** - HashMap lookup + update - O(1)

## Endpoints

- `GET /.well-known/agent.json` - Agent card
- `POST /a2a/tasks` - Create task
- `GET /a2a/tasks/{id}` - Get task status
