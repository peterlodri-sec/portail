# Gateway Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     AI Gateway Forwarding                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Client Request                                                │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  strip_    │────▶│  add_xff() │────▶│  HTTP      │         │
│   │  hop_by_   │     │            │     │  Client    │         │
│   │  hop()     │     │            │     │  (reqwest) │         │
│   └────────────┘     └────────────┘     └────────────┘         │
│                                              │                  │
│                                              ▼                  │
│                                       ┌────────────┐            │
│                                       │  Upstream  │            │
│                                       │  (LiteLLM) │            │
│                                       └────────────┘            │
│                                              │                  │
│                                              ▼                  │
│                                       ┌────────────┐            │
│                                       │  Response  │            │
│                                       │  (strip)   │            │
│                                       └────────────┘            │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Hop-by-Hop Headers Removed                                    │
│                                                                 │
│   host, connection, transfer-encoding, proxy-authenticate,      │
│   proxy-authorization, te, trailer, upgrade, keep-alive         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **forward()** - Parse request, forward, return response - O(1)
2. **forward_with_body()** - Same but with pre-read body - O(n)
3. **strip_hop_by_hop()** - Filter headers - O(n)

## Client Configuration

```rust
Client::builder()
    .timeout(Duration::from_secs(600))
    .http2_keep_alive_interval(Some(Duration::from_secs(30)))
    .build()
```
