# Architecture Decision Records

> Key architectural decisions made during Portail's development.
> Each ADR records a decision, its context, and the alternatives considered.

---

## ADR-001: Single Binary Approach

**Status:** Accepted
**Date:** 2026-06-01

### Context

Portail started as a proxy for AI services. The question was whether to build
a single monolithic binary or a suite of microservices (separate proxy, cache,
monitoring, DNS binaries).

### Decision

Ship as a **single Rust binary** (`portail` binary, `portail serve` for server
mode, zero-arg for TUI). Internal modules are organized as a library with
feature flags for optional backends (Turso, jemalloc).

### Consequences

**Positive:**
- One deployment artifact — copy one binary, done
- Shared types and state across modules (AppState)
- No IPC overhead between subsystems
- Simpler configuration — one config file

**Negative:**
- All dependencies bundled — larger binary (~15MB compressed)
- A crash in any subsystem takes down all subsystems
- Cannot independently scale subsystems

---

## ADR-002: axum as HTTP Framework

**Status:** Accepted
**Date:** 2026-06-01

### Context

Portail needed an HTTP framework that supports middleware, streaming, WebSockets,
and high throughput. Contenders: axum, actix-web, warp, hyper directly.

### Decision

Use **axum 0.8** as the HTTP framework.

### Rationale

- First-class async support via tokio (which Portail already uses)
- Tower middleware ecosystem (auth, rate-limit, tracing, CORS as layers)
- Extractors integrate naturally with Rust's async model
- WebSocket support built-in (used by A2A protocol)
- OpenAPI integration via utoipa-axum

---

## ADR-003: OpenAI-Compatible Canonical Format

**Status:** Accepted
**Date:** 2026-06-27

### Context

Portail proxies requests to multiple AI providers (OpenAI, DeepSeek, Anthropic,
Google, Ollama). Each has a different API schema. Clients sending requests in
one format need responses transformed to match.

### Decision

Use **OpenAI's chat completion format** as the canonical internal format.
All client requests are expected in OpenAI format. Provider adapters transform
to/from provider-specific formats at the gateway layer.

### Consequences

- Clients only need to know one API format
- Provider adapters are isolated and independently testable
- Adding a new provider = implementing `ProviderAdapter` trait
- Feature virtualizer (`features.rs`) handles unsupported features per provider

---

## ADR-004: TOML Configuration (Figment)

**Status:** Accepted
**Date:** 2026-06-01

### Context

Portail needed a configuration system that supports file-based config, env vars,
and sensible defaults — with hot-reload support.

### Decision

Use **TOML** as the config format, loaded via **figment** with stacked providers:

1. Struct defaults
2. TOML file (`portail.toml` or `/etc/portail/config.toml`)
3. Environment variables (`PORTAIL_*` prefix)
4. CLI arguments

Hot-reload is handled separately via `config_watcher.rs` (inotify/FSEvents).

---

## ADR-005: Moka + blake3 for CDN Cache

**Status:** Accepted
**Date:** 2026-06-01

### Context

Portail's CDN cache needed low-latency reads for hot content and large-capacity
storage for cold content.

### Decision

Two-tier cache:
- **Tier 1**: Moka (in-memory, concurrent, TTL-based) for hot content
- **Tier 2**: blake3 content-addressed disk cache with mmap zero-copy reads

Cache key is the blake3 hash of the URL + request headers.

---

## ADR-006: Forbid Unsafe Code

**Status:** Accepted
**Date:** 2026-06-01

### Context

Portail handles untrusted input (HTTP requests, upstream responses, config files).
Memory safety is critical.

### Decision

Enforce `#![forbid(unsafe_code)]` at the crate root. Zero `unsafe` blocks
allowed — enforced by the compiler.

This extends to all workspace crates (loopeng, pkg-ctx, portail-agents, etc.).

---

## ADR-007: Git Hooks for CI Feedback Loop

**Status:** Accepted
**Date:** 2026-06-27

### Context

Contributors need fast feedback on code quality without waiting for CI.

### Decision

Provide a pre-commit hook that runs:
1. `cargo fmt --check` — formatting
2. `cargo clippy --all-targets -- -D warnings` — linting
3. `cargo check --locked` — compilation

Installable via `task install-hooks` or the contributor setup script.

---

## ADR-008: Utoipa for Auto-Generated OpenAPI Spec

**Status:** Accepted
**Date:** 2026-06-27

### Context

Portail has 60+ API endpoints across multiple modules. Manually maintained
OpenAPI specs drift from implementation.

### Decision

Use **utoipa** + **utoipa-axum** for compile-time OpenAPI 3.1 spec generation
from `#[utoipa::path(...)]` handler annotations. Serve via **Scalar UI** at
`/api-docs/`.

Routes without annotations still work — they just won't appear in the spec.
Annotating a handler takes ~30 seconds and the spec updates automatically.

---

## ADR-009: Provider Feature Virtualization

**Status:** Accepted (v2.3)
**Date:** 2026-06-27

### Context

Different AI providers support different feature sets. A client that sends
`response_format` to Anthropic (which doesn't support it) gets an error.

### Decision

Implement a **capability matrix** (`features.rs`) that declares what each
provider supports. For unsupported features, apply fallback strategies:

- **Strip + Warn**: silently remove the field (e.g., `seed` on Anthropic)
- **Prompt Injection**: inject instructions into the system prompt
  (e.g., "Respond in JSON format" for `response_format`)
- **Response Transform**: post-process the response to simulate the feature
  (e.g., detect tool calls in plain text for Ollama)

---

## ADR-010: Config File Watcher with Rollback

**Status:** Accepted
**Date:** 2026-06-01

### Context

Portail operators need to change configuration without restarting the server.

### Decision

Use `notify` crate for filesystem events. On config file change:
1. Debounce (500ms) to coalesce rapid edits
2. Validate the new config
3. Atomic swap via `Arc<RwLock<Config>>`
4. On validation failure: keep current config + log error + rollback

The previous valid config is always preserved for rollback.
