# Sentinel Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Sentinel Health Watcher                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                    30s Tick Loop                         │   │
│   └─────────────────────────────────────────────────────────┘   │
│                           │                                     │
│                           ▼                                     │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  1. CDN Scrub Stats                                     │   │
│   │     - evictions, entries, size                           │   │
│   │                                                         │   │
│   │  2. Health Check                                        │   │
│   │     - proxy, cdn, mcp, events status                    │   │
│   │                                                         │   │
│   │  3. Publish Events                                      │   │
│   │     - event_log.publish(cdn_scrub)                      │   │
│   │     - event_log.publish(health_check)                   │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Event Types Published                                         │
│                                                                 │
│   - "started"       - Sentinel initialized with PID             │
│   - "cdn_scrub"     - CDN eviction/entry counts                 │
│   - "health_check"  - Subsystem status (proxy/cdn/mcp/events)   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **run_sentinel()** - Tick loop - O(1) per iteration
2. **publish()** - Event log write - O(1)

## Configuration

- Tick interval: 30 seconds
- Agent ID: "sentinel"
- Event log: Shared with main AppState
