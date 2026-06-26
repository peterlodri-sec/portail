# Portail v0.2.0 Tasks

## Overview

Version 0.2.0 focuses on production hardening, advanced agent features, and ecosystem integration.

**Target**: Q3 2026

---

## High Priority

### 1. OpenTelemetry Export (OTLP)
- [ ] Add `opentelemetry-otlp` crate
- [ ] Export traces to Jaeger/Tempo
- [ ] Export metrics to Prometheus OTLP
- [ ] Auto-instrument axum handlers
- **Issue**: #XXX
- **Effort**: 3 days

### 2. Rate Limiting
- [ ] Token bucket per API key/IP
- [ ] Sliding window counters
- [ ] Configurable limits per endpoint
- [ ] 429 responses with Retry-After header
- **Issue**: #XXX
- **Effort**: 2 days

### 3. Authentication Middleware
- [ ] API key validation (Bearer token)
- [ ] JWT verification (RS256/ES256)
- [ ] Per-route auth requirements
- [ ] Auth bypass for health/metrics
- **Issue**: #XXX
- **Effort**: 3 days

### 4. Persistent Event Store
- [ ] SQLite backend for event log
- [ ] Configurable retention policy
- [ ] Query by agent_id, event_type, time range
- [ ] Export to JSON/CSV
- **Issue**: #XXX
- **Effort**: 2 days

---

## Medium Priority

### 5. WebSocket Support
- [ ] Upgrade HTTP to WebSocket
- [ ] Bidirectional streaming for A2A
- [ ] Connection management (ping/pong)
- [ ] Message framing
- **Issue**: #XXX
- **Effort**: 3 days

### 6. GraphQL API
- [ ] Add `async-graphql` crate
- [ ] Schema for events, hooks, tasks
- [ ] Subscriptions for live updates
- [ ] Playground UI
- **Issue**: #XXX
- **Effort**: 4 days

### 7. Plugin System
- [ ] Dynamic plugin loading (WASM or native)
- [ ] Plugin API (hooks, routes, events)
- [ ] Plugin registry
- [ ] Sandboxing
- **Issue**: #XXX
- **Effort**: 5 days

### 8. Multi-tenant Support
- [ ] Tenant isolation (separate event logs, hooks)
- [ ] Per-tenant rate limits
- [ ] Tenant-scoped API keys
- [ ] Usage tracking per tenant
- **Issue**: #XXX
- **Effort**: 4 days

### 9. Advanced Caching
- [ ] Cache warming (pre-fetch popular endpoints)
- [ ] Cache invalidation webhooks
- [ ] Cache tags (group invalidation)
- [ ] Cache compression (zstd)
- **Issue**: #XXX
- **Effort**: 3 days

---

## Low Priority

### 10. Admin Dashboard (Web UI)
- [ ] React/Svelte frontend
- [ ] Real-time event stream
- [ ] Hook management UI
- [ ] Cache statistics
- [ ] Configuration editor
- **Issue**: #XXX
- **Effort**: 5 days

### 11. Distributed Tracing
- [ ] W3C Trace Context propagation
- [ ] Span correlation across services
- [ ] Trace sampling (head/tail)
- [ ] Trace visualization
- **Issue**: #XXX
- **Effort**: 3 days

### 12. HeadScale Integration
- [ ] Replace Tailscale with HeadScale
- [ ] Self-hosted control plane
- [ ] DNS integration
- [ ] Certificate management
- **Issue**: #XXX
- **Effort**: 4 days

### 13. Advanced Agent Features
- [ ] Agent registry (discovery, capabilities)
- [ ] Agent-to-agent messaging (pub/sub)
- [ ] Agent task queues
- [ ] Agent health monitoring
- **Issue**: #XXX
- **Effort**: 5 days

### 14. Performance Tuning
- [ ] Benchmark suite (criterion)
- [ ] Memory profiling (dhat)
- [ ] CPU profiling (perf, flamegraph)
- [ ] Load testing (k6, vegeta)
- **Issue**: #XXX
- **Effort**: 2 days

---

## Documentation

### 15. API Reference
- [ ] OpenAPI 3.1 spec
- [ ] Auto-generated from code
- [ ] Interactive playground
- [ ] Client SDKs (Python, TypeScript)
- **Issue**: #XXX
- **Effort**: 3 days

### 16. Architecture Guide
- [ ] Module dependency graph
- [ ] Data flow diagrams
- [ ] Security model documentation
- [ ] Deployment guides (Nix, Docker, systemd)
- **Issue**: #XXX
- **Effort**: 2 days

### 17. Tutorials
- [ ] Getting started guide
- [ ] Hook injection tutorial
- [ ] A2A agent setup
- [ ] Multi-agent workflow
- **Issue**: #XXX
- **Effort**: 3 days

---

## Infrastructure

### 18. CI/CD Improvements
- [ ] Benchmark regression detection
- [ ] Code coverage reporting
- [ ] Dependency audit automation
- [ ] Release automation (cargo-release)
- **Issue**: #XXX
- **Effort**: 2 days

### 19. Testing
- [ ] Integration test suite
- [ ] Load test harness
- [ ] Chaos testing (network partitions)
- [ ] Security fuzzing (cargo-fuzz)
- **Issue**: #XXX
- **Effort**: 3 days

### 20. Packaging
- [ ] Homebrew formula
- [ ] AUR package
- [ ] Debian package (cargo-deb)
- [ ] RPM package
- [ ] Snap package
- [ ] Flatpak package
- **Issue**: #XXX
- **Effort**: 2 days

---

## Summary

| Priority | Tasks | Total Effort |
|----------|-------|--------------|
| High | 4 | 10 days |
| Medium | 5 | 19 days |
| Low | 5 | 19 days |
| Docs | 3 | 8 days |
| Infra | 3 | 7 days |
| **Total** | **20** | **63 days** |

---

## Dependencies

```
Rate Limiting → Authentication
Authentication → Multi-tenant
OpenTelemetry → Distributed Tracing
Plugin System → Advanced Agent Features
Persistent Events → GraphQL API
```

---

## Release Criteria

- [ ] All high-priority tasks complete
- [ ] 90%+ test coverage
- [ ] No known security vulnerabilities
- [ ] API documentation complete
- [ ] Performance benchmarks pass
- [ ] 100+ tests passing
- [ ] Clippy clean with `-D warnings`
- [ ] All CI workflows passing

---

## How to Contribute

1. Pick a task from the list
2. Create an issue: `gh issue create --title "v0.2: Task name"`
3. Create a branch: `git checkout -b feature/task-name`
4. Implement and test
5. Submit PR: `gh pr create`

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.
