# Events Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       Event Log Flow                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Agent Action                                                  │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  publish() │────▶│  Ring      │────▶│  Broadcast │         │
│   │            │     │  Buffer    │     │  Channel   │         │
│   └────────────┘     │  (2000)    │     │  (2048)    │         │
│                      └────────────┘     └────────────┘         │
│                           │                   │                 │
│                           ▼                   ▼                 │
│                      recent(n)          SSE Stream              │
│                      GET /events        GET /events/stream      │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Ring Buffer (VecDeque)                                        │
│                                                                 │
│   ┌───┬───┬───┬───┬───┬───┬───┬───┐                            │
│   │ 0 │ 1 │ 2 │ 3 │ 4 │ 5 │ 6 │ 7 │  ← max 2000 events       │
│   └───┴───┴───┴───┴───┴───┴───┴───┘                            │
│     ▲           │                                               │
│     └───────────┘ oldest removed when full                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **publish()** - Lock ring, push, broadcast - O(1)
2. **recent()** - Lock ring, iterate reverse - O(n)
3. **subscribe()** - Create broadcast receiver - O(1)

## Data Flow

```
publish() → ring.push_back() → broadcast.send() → SSE stream
                 │
                 └─→ recent() → GET /events
```

## Dependencies

- `tokio::sync::broadcast` - Multi-consumer channel
- `std::collections::VecDeque` - Ring buffer
- `rustc_hash::FxHashMap` - Fast metadata storage
