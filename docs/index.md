---
title: Portail — Unified Proxy / Gateway
layout: home
---

# Portail

Unified proxy that bundles AI Gateway, MCP Gateway, and CDN Cache behind a
single port.

## Quick links

- [GitHub repository](https://github.com/peterlodri-sec/portail)
- [README](https://github.com/peterlodri-sec/portail#readme) — full docs
- [Crates.io](https://crates.io/crates/portail)
- [Docs.rs](https://docs.rs/portail)
- [Issue tracker](https://github.com/peterlodri-sec/portail/issues)

## Install

```bash
cargo install portail
```

Or via Nix:

```bash
nix run github:peterlodri-sec/portail
```

## Quick start

```bash
portail --config /etc/portail/config.toml
```

## Architecture

```
                           ┌─────────────┐
  ──▶  AI  API calls ───▶  │             │──▶ LiteLLM / upstream
  ──▶  MCP  tool calls ──▶  │   Portail   │──▶ Python MCP sidecar
  ──▶  CDN  asset fetches ▶  │   :8787    │──▶ S3 / MinIO origin
                           └─────────────┘
```

Built with Rust (axum, tokio, moka, blake3) and a Python MCP sidecar.
