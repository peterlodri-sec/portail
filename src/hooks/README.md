# Hooks Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Hook Injection                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Incoming Request                                              │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  match()   │────▶│  apply()   │────▶│  forward() │         │
│   │            │     │            │     │            │         │
│   └────────────┘     └────────────┘     └────────────┘         │
│        │                   │                                    │
│        ▼                   ▼                                    │
│   ┌────────────┐     ┌────────────┐                             │
│   │  HookStore │     │  Inject    │                             │
│   │  (RwLock)  │     │  Prepend/  │                             │
│   └────────────┘     │  Append    │                             │
│                      └────────────┘                             │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Hook Matching                                                 │
│                                                                 │
│   match_message(path) → hooks where path.contains(match_path)   │
│   match_event(agent, type) → hooks where agent + type match     │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Injection Modes                                               │
│                                                                 │
│   Prepend: [system_msg] + [user_msg] + [assistant_msg]         │
│   Append:  [user_msg] + [assistant_msg] + [system_msg]         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **match_message()** - Read lock, filter hooks - O(n)
2. **apply_message_hooks()** - Clone body, insert messages - O(n)
3. **add()** - Write lock, push - O(1)

## Configuration

```json
{
  "id": "hook-1",
  "match_path": "/chat",
  "inject": "prepend",
  "content": "You are a helpful assistant.",
  "enabled": true
}
```
