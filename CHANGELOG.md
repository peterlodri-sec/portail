# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- WebGL demo page (docs/demo.html)
- .editorconfig for consistent coding styles
- .gitattributes for line ending normalization
- CODE_OF_CONDUCT.md
- Enhanced .gitignore

## [0.1.0] - 2026-06-26

### Added
- AI Gateway (OpenAI/Anthropic/LiteLLM proxy)
- MCP Gateway (Unix socket sidecar)
- CDN Cache (Moka + blake3 filesystem)
- A2A Protocol (Agent-to-Agent)
- A2C Interface (Agent-to-Consumer)
- Hook Injection (per-message/per-event)
- Event Log (ring buffer + SSE)
- Sentinel (health monitoring)
- NullClaw (network-native agent)
- Godfather (service monitor)
- Discovery (self-service network discovery)
- CI Status Webhook (live badge)
- TUI Dashboard (network visualization)
- DNS (DoH + network isolation)
- TinyURL (auto URL shortening)
- Tracer (request/response E2E)
- Redis Cache (app-level)
- eBPF Observability
- io_uring Async I/O
- DPDK Kernel Bypass
- Hyper Low-Level HTTP
- Self-hosted runners on dev-cx53
- GitHub Actions with fork detection
- Cosign-signed releases
- Docker multi-arch builds
- NixOS module with hardening
- HSTS + security headers
- 107 tests passing
