# CDN Cache Module

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        CDN Cache Flow                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Client Request                                                │
│        │                                                        │
│        ▼                                                        │
│   ┌────────────┐     ┌────────────┐     ┌────────────┐         │
│   │  Moka      │────▶│  Blake3    │────▶│  Origin    │         │
│   │  (Memory)  │ miss │  (Disk)   │ miss │  (MinIO)   │         │
│   └────────────┘     └────────────┘     └────────────┘         │
│        │ hit              │ hit              │                  │
│        └──────────────────┴──────────────────┘                  │
│                    │                                            │
│                    ▼                                            │
│              Return Response                                    │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│   NATS Invalidation                                             │
│                                                                 │
│   index.invalidated.{key} ──▶ purge_loop ──▶ cache.purge()     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hot Paths

1. **Cache lookup** - `cache.get(key)` - O(1) moka lookup
2. **Blake3 hash** - `blake3::hash(key)` - SIMD-optimized
3. **Disk read** - `fs::read(path)` - Memory-mapped for large files

## Dependencies

- `moka` - Concurrent cache with TTL
- `blake3` - SIMD-optimized hashing
- `tokio` - Async I/O
- `async-nats` - NATS invalidation

## Configuration

```toml
[cdn]
enabled = true
origin = "http://127.0.0.1:9000"
cache_dir = "/var/cache/portail"
cache_size = "10g"
nats_url = "nats://localhost:4222"
```
