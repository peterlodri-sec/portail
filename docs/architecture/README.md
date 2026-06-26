# Portail Documentation

## Table of Contents

- [Quick Start](../README.md#quick-start)
- [Configuration](../README.md#configuration)
- [API Reference](../README.md#api-endpoints)
- [Architecture](../DESIGN.md)

## Modules

| Module | Description | File |
|--------|-------------|------|
| proxy | HTTP routing, middleware | `src/proxy.rs` |
| gateway | AI upstream forwarding | `src/gateway/mod.rs` |
| cdn | Two-tier cache | `src/cdn/mod.rs` |
| events | Event log + SSE | `src/events/mod.rs` |
| hooks | Prompt injection | `src/hooks/mod.rs` |
| sentinel | Health watcher | `src/sentinel/mod.rs` |
| mcp | MCP sidecar | `src/mcp/mod.rs` |
| a2a | Agent-to-Agent protocol | `src/a2a/mod.rs` |
| a2c | Agent-to-Consumer chat | `src/a2c/mod.rs` |
| cli | TUI dashboard + CLI | `src/cli/` |

## Building

```bash
# Debug build
cargo build

# Release build (LTO + strip)
cargo build --release

# With UPX compression
upx --best --lzma target/release/portail
```

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

## Deployment

### NixOS

```nix
# flake.nix
inputs.portail.url = "github:peterlodri-sec/portail";

# configuration.nix
services.portail = {
  enable = true;
  package = inputs.portail.packages.${pkgs.system}.portail;
};
```

### Docker

```bash
docker build -t portail .
docker run -p 8787:8787 portail
```

### Systemd

```ini
[Unit]
Description=Portail proxy
After=network.target

[Service]
ExecStart=/usr/local/bin/portail serve
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```
