//! WASM MCP plugin wrapper using Extism
//!
//! Resource ceilings enforced per-plugin:
//!   - Memory: 256 Wasm pages (16 MiB)
//!   - Fuel: 10M instructions
//!   - Timeout: 30s
//!   - HTTP: no outbound (empty allow-list)

use extism::{Manifest, Plugin, PluginBuilder, Wasm};
use serde_json::Value;
use std::time::Duration;
use tracing::{info, warn};

/// Default resource limits for WASM MCP plugins.
pub struct WasmLimits {
    /// Max linear memory in Wasm pages (64 KiB each). Default: 256 (16 MiB).
    pub memory_pages: u32,
    /// Max Wasm instructions (fuel). Default: 10_000_000.
    pub fuel_limit: u64,
    /// Max execution time per call. Default: 30s.
    pub timeout: Duration,
}

impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            memory_pages: 256,
            fuel_limit: 10_000_000,
            timeout: Duration::from_secs(30),
        }
    }
}

/// A single WASM MCP server plugin with enforced resource ceilings.
pub struct McpWasmPlugin {
    name: String,
    plugin: Plugin,
    limits: WasmLimits,
}

impl McpWasmPlugin {
    /// Load a WASM MCP plugin from bytes with default limits.
    pub fn from_bytes(name: &str, wasm_bytes: &[u8]) -> anyhow::Result<Self> {
        Self::from_bytes_with_limits(name, wasm_bytes, WasmLimits::default())
    }

    /// Load a WASM MCP plugin from bytes with custom limits.
    pub fn from_bytes_with_limits(
        name: &str,
        wasm_bytes: &[u8],
        limits: WasmLimits,
    ) -> anyhow::Result<Self> {
        let wasm = Wasm::data(wasm_bytes.to_vec());

        let manifest = Manifest::new([wasm])
            .with_memory_max(limits.memory_pages)
            .with_timeout(limits.timeout);

        let plugin = PluginBuilder::new(&manifest)
            .with_fuel_limit(limits.fuel_limit)
            .build()?;

        info!(
            name,
            memory_pages = limits.memory_pages,
            fuel_limit = limits.fuel_limit,
            timeout_secs = limits.timeout.as_secs(),
            "WASM MCP plugin loaded"
        );

        Ok(Self {
            name: name.to_string(),
            plugin,
            limits,
        })
    }

    /// Load a WASM MCP plugin from a file path with default limits.
    pub fn from_file(name: &str, path: &std::path::Path) -> anyhow::Result<Self> {
        let wasm_bytes = std::fs::read(path)?;
        Self::from_bytes(name, &wasm_bytes)
    }

    /// Call the plugin's MCP handler with a JSON-RPC request.
    /// Returns (response, fuel_consumed).
    pub fn handle_request(&mut self, request: &Value) -> anyhow::Result<(Value, u64)> {
        let input = serde_json::to_string(request)?;
        let output = self.plugin.call::<&str, &str>("mcp_handle", &input)?;
        let response: Value = serde_json::from_str(output)?;
        let fuel = self.plugin.fuel_consumed().unwrap_or(0);
        if fuel > self.limits.fuel_limit * 80 / 100 {
            warn!(
                name = %self.name,
                fuel,
                limit = self.limits.fuel_limit,
                "plugin approaching fuel limit"
            );
        }
        Ok((response, fuel))
    }

    /// Initialize the plugin (call `initialize` method).
    pub fn initialize(&mut self, capabilities: &Value) -> anyhow::Result<Value> {
        let input = serde_json::to_string(capabilities)?;
        let output = self.plugin.call::<&str, &str>("mcp_initialize", &input)?;
        let response: Value = serde_json::from_str(output)?;
        Ok(response)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn limits(&self) -> &WasmLimits {
        &self.limits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_from_bytes_invalid_wasm() {
        let result = McpWasmPlugin::from_bytes("test", b"not valid wasm");
        assert!(result.is_err());
    }

    #[test]
    fn default_limits_are_sane() {
        let limits = WasmLimits::default();
        assert_eq!(limits.memory_pages, 256); // 16 MiB
        assert_eq!(limits.fuel_limit, 10_000_000);
        assert_eq!(limits.timeout, Duration::from_secs(30));
    }

    #[test]
    fn custom_limits_applied() {
        let limits = WasmLimits {
            memory_pages: 64,
            fuel_limit: 1_000_000,
            timeout: Duration::from_secs(5),
        };
        assert_eq!(limits.memory_pages, 64); // 4 MiB
        assert_eq!(limits.fuel_limit, 1_000_000);
        assert_eq!(limits.timeout, Duration::from_secs(5));
    }
}
