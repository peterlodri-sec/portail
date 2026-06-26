# Email to Rust Optimization Author

**To:** [Rust optimization blog author]

**Subject:** Portail — native Rust proxy, would value your lens on it

Hey,

I built Portail — a unified AI/MCP/CDN proxy in Rust (axum, moka, blake3,
zero-copy MCP framing). Your blog posts on Rust optimization have been a
staple reference, and I'd be honored if you'd take a look.

The architecture is deliberately minimal: a single axum router with three
subsystem handlers behind `Arc<RwLock<Config>>`, streaming passthrough for
AI calls, two-tier (moka → blake3-filesystem) CDN cache with NATS
invalidation, and a Python MCP sidecar over a framed Unix socket protocol.

I'm particularly interested in your thoughts on:

- The MCP framing layer — uses `bytes::BufMut` zero-copy encoding, currently
  allocating `Vec<u8>` on decode. Could a read-once `Bytes`-based decoder
  tighten the hot path?
- moka cache entry sizing — currently pinning `max_entries` via
  `max_capacity / 1_000_000`. Is there a more principled approach?
- The axum `TraceLayer` + `metrics` crate combination — emitting structured
  JSON logs and prometheus histograms on every request. Any patterns you've
  seen that reduce allocation pressure under load?

Would be happy to jump on a call to walk through the design or pair on
optimizations. The repo is at `github.com/peterlodri-sec/portail`.

Thanks for your time.
