# DNS Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      DNS Resolution Flow                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   DNS Query                                                     │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  Apply     │────▶│  Local     │────▶│  DoH       │         │
│   │  Hooks     │     │  Store     │     │  Client    │         │
│   └────────────┘     └────────────┘     └────────────┘         │
│        │                   │ hit              │                 │
│        │                   └──────────────────┘                 │
│        │                          │                             │
│        ▼                          ▼                             │
│   ┌────────────┐           ┌────────────┐                       │
│   │  Block/    │           │  Response  │                       │
│   │  Redirect  │           │            │                       │
│   └────────────┘           └────────────┘                       │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   DoH Endpoints                                                 │
│                                                                 │
│   Cloudflare: https://cloudflare-dns.com/dns-query              │
│   Google:     https://dns.google/dns-query                      │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   Network Isolation                                             │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  Allowed Domains: [example.com, api.example.com]        │   │
│   │  Blocked Domains: [ads.example.com, tracker.com]        │   │
│   │  Allowed IPs:     [10.0.0.0/8, 192.168.0.0/16]         │   │
│   │  Blocked IPs:     [malicious.ip.address]                │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **apply_hooks()** - Pattern match on domain - O(n)
2. **query()** - Local store lookup - O(1)
3. **doh.query()** - HTTPS request to DoH server - O(1)

## Configuration

```toml
[dns]
enabled = true
listen = "127.0.0.1:53"
doh_enabled = true
doh_endpoints = ["https://cloudflare-dns.com/dns-query"]

[dns.hooks]
id = "block-ads"
pattern = "ads.example.com"
action = "block"
```
