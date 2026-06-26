# Network Layers — Portail Deep Dive

> See also: [architecture/NETWORK_DESIGN.md](architecture/NETWORK_DESIGN.md) for the full architecture.

---

## OSI Model Quick Reference

| Layer | Name | What | Portail Uses |
|-------|------|------|--------------|
| 7 | Application | HTTP, DNS, TLS | axum, reqwest, DoH |
| 6 | Presentation | Encryption, encoding | TLS, JSON, base62 |
| 5 | Session | Connections | HTTP/2 keep-alive |
| 4 | Transport | TCP, UDP | tokio TCP |
| 3 | Network | IP routing | IP headers |
| 2 | Data Link | Ethernet | N/A |
| 1 | Physical | Cables | N/A |

Portail operates primarily at **Layer 7** (Application) and **Layer 4** (Transport).

---

## Request Flow Through Layers

```
Client                    Portail                    Upstream
  │                          │                          │
  │  POST /v1/chat           │                          │
  │─────────────────────────>│                          │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ L7: HTTP │  axum router       │
  │                     │ Route    │  middleware stack   │
  │                     └────┬────┘                     │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ L7: Hook│  Inject prompts     │
  │                     │ Inject  │                      │
  │                     └────┬────┘                     │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ L6: TLS │  Encrypt to         │
  │                     │ Connect │  upstream            │
  │                     └────┬────┘                     │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ L4: TCP │  tokio TCP stream   │
  │                     │ Connect │                      │
  │                     └────┬────┘                     │
  │                          │                          │
  │                          │  POST /v1/chat           │
  │                          │─────────────────────────>│
  │                          │                          │
  │                          │  200 OK                  │
  │                          │<─────────────────────────│
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ L7: Cache│ Store response     │
  │                     │ Write    │ (Moka + disk)      │
  │                     └────┬────┘                     │
  │                          │                          │
  │                     ┌────┴────┐                     │
  │                     │ L7: Trace│ Record span        │
  │                     │ Record   │ (OTLP export)      │
  │                     └────┬────┘                     │
  │                          │                          │
  │  200 OK                  │                          │
  │<─────────────────────────│                          │
```

---

## Middleware Layer Stack (In Order)

```
Request
  │
  ▼
┌─────────────────┐
│ CORS            │ Cross-Origin Resource Sharing
├─────────────────┤
│ Rate Limit      │ Token bucket (governor), 429 + Retry-After
├─────────────────┤
│ Auth            │ JWT / API-key, bypass list for health/metrics
├─────────────────┤
│ Session         │ Per-session request recording (x-session-id)
├─────────────────┤
│ TraceLayer      │ HTTP tracing (tower-http)
├─────────────────┤
│ Body Limit      │ 10MB cap
├─────────────────┤
│ Metrics         │ Prometheus counters + histograms
├─────────────────┤
│ Request ID      │ x-request-id injection + propagation
├─────────────────┤
│ Security Headers│ HSTS, CSP, X-Frame-Options, etc.
├─────────────────┤
│ Route Handler   │ Matched endpoint handler
└─────────────────┘
```

---

## DNS Layer

```
What happens when you type "example.com":

1. Browser asks: "What IP is example.com?"
2. OS checks local cache → miss
3. OS asks resolver (1.1.1.1 or 8.8.8.8)
4. Resolver asks root servers: "Who handles .com?"
5. Root says: "Ask the .com TLD servers"
6. Resolver asks TLD: "Who handles example.com?"
7. TLD says: "Ask ns1.example.com"
8. Resolver asks authoritative: "What IP for example.com?"
9. Authoritative says: "93.184.216.34"
10. Browser connects to 93.184.216.34

Portail adds:
- DNS over HTTPS (DoH) — encrypts step 3
- DNS cache — TTL-aware, negative caching
- Fallback resolvers — Cloudflare → Google → OpenDNS chain
- Network isolation — controls which domains are allowed
```

---

## TLS Layer

```
How HTTPS works:

Client                              Server
  │                                    │
  │  "Hello, I support TLS 1.3"       │
  │───────────────────────────────────>│
  │                                    │
  │  "Hello, here's my certificate"   │
  │<───────────────────────────────────│
  │                                    │
  │  [Verify certificate]             │
  │  [Generate session key]           │
  │                                    │
  │  "Let's use this key"             │
  │───────────────────────────────────>│
  │                                    │
  │  [Encrypted communication]        │
  │<──────────────────────────────────>│

Portail supports:
- Self-signed (development)
- Let's Encrypt (production, planned v2.0)
- Custom certificates
```

---

## Cache Layer

```
Two-tier cache architecture:

Tier 1: Moka (in-memory)
  - LRU eviction
  - TTL-aware
  - <1ms lookup
  - Shared across requests

Tier 2: cacache (disk)
  - Content-addressable (blake3 hash)
  - mmap for zero-copy reads
  - ~5ms lookup
  - Survives restart

Invalidation: NATS pub/sub (opt-in)
  - "index.invalidated.>" subjects
  - Multi-node consistency
```
