# MCP Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    MCP Sidecar Proxy                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Client Request                                                │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  HTTP      │────▶│  Unix      │────▶│  Python    │         │
│   │  Request   │     │  Socket    │     │  Sidecar   │         │
│   └────────────┘     │  (framed)  │     │            │         │
│                      └────────────┘     └────────────┘         │
│                           │                   │                 │
│                           ▼                   ▼                 │
│                      ┌────────────┐     ┌────────────┐         │
│                      │  Decode    │◀────│  Encode    │         │
│                      │  Response  │     │  Response  │         │
│                      └────────────┘     └────────────┘         │
│                           │                                     │
│                           ▼                                     │
│                      HTTP Response                              │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Framed Protocol                                               │
│                                                                 │
│   ┌──────────┬──────────┬──────────┬──────────┬──────────┐      │
│   │ method   │ method   │ path     │ headers  │ body     │      │
│   │ len:u16  │ bytes    │ len:u32  │ len:u32  │ len:u64  │      │
│   └──────────┴──────────┴──────────┴──────────┴──────────┘      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **proxy_to_sidecar()** - Serialize, send, receive, deserialize - O(n)
2. **encode/decode** - Binary framing - O(n)

## Protocol

```
[method_len:u16][method:bytes][path_len:u32][path:bytes]
[headers_len:u32][headers:json][body_len:u64][body:bytes]
```
