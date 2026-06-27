//! portail-mcp-wasm — WASM-based MCP sidecar for Portail
//!
//! Replaces the Python `uv` sidecar with Extism WASM runtime.
//! Maintains the same Unix socket + binary framing protocol for backwards compatibility.

pub mod plugin;
pub mod server;

pub use server::WasmMcpServer;
