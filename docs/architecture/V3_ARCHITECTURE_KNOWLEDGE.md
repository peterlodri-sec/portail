# v3.0 Architecture Knowledge — Go + Rust + Wasm Hybrid

## Research Summary (2026-06-26)

Based on SOTA patterns for combining Go and Rust in production systems:

### Integration Patterns
1. **IPC (Apache Arrow / Unix Sockets + NDJSON)** — loose coupling, best for agent/TUI systems
2. **CGO Static Linking** — sub-microsecond latency, passes C pointers, 50-100ns overhead
3. **Wazero / Wasmtime** — sandboxed plugin architecture, WASI syscall interception

### Native Wasm Agents
- **Extism PDK** — plug-in development kit for host/guest memory mapping
- **Wasi Preview 2** — standardized kernel abstraction for POSIX in Wasm
- **Fuel Metering** — CPU instruction limits prevent infinite agent loops
- **Memory**: 30-50MB runtime, 1ms cold-start, near-native speed via AOT compilation

### Wasm Size Optimization
- `lto = true, panic = "abort", opt-level = "z", codegen-units = 1`
- `lol_alloc` (FreeListAllocator) replaces dlmalloc: 150KB → 15-30KB
- `wasm-opt -Oz` post-processing strips redundant metadata
- Nix: `pkgs.binaryen` hooks optimization into build phase

### Go Architecture Patterns
- Functional domain packaging (not technical layers)
- Accept interfaces, return concrete types
- Zero-allocation sync.Pool for hot paths
- errgroup + context for graceful shutdown orchestration
- sqlc for type-safe SQL, sonic for high-speed JSON

### Reference Implementations
- `github.com/extism/go-sdk` — Go host for Wasm plugins
- `github.com/tetratelabs/wazero` — pure-Go Wasm runtime (zero CGO)
- `github.com/bytecodealliance/wasmtime-go` — Rust-native Wasm engine
- Apache Arrow IPC — zero-copy cross-process streaming (arrow-rs crate)
