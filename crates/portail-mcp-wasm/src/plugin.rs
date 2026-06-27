//! WASM MCP plugin wrapper using Extism

use extism::{Manifest, Plugin, Wasm};
use serde_json::Value;
use tracing::info;

/// A single WASM MCP server plugin
pub struct McpWasmPlugin {
    name: String,
    plugin: Plugin,
}

impl McpWasmPlugin {
    /// Load a WASM MCP plugin from bytes
    pub fn from_bytes(name: &str, wasm_bytes: &[u8]) -> anyhow::Result<Self> {
        let wasm = Wasm::data(wasm_bytes.to_vec());
        let manifest = Manifest::new([wasm]);
        let plugin = Plugin::new(&manifest, [], true)?;

        info!(name, "WASM MCP plugin loaded");
        Ok(Self {
            name: name.to_string(),
            plugin,
        })
    }

    /// Load a WASM MCP plugin from a file path
    pub fn from_file(name: &str, path: &std::path::Path) -> anyhow::Result<Self> {
        let wasm_bytes = std::fs::read(path)?;
        Self::from_bytes(name, &wasm_bytes)
    }

    /// Call the plugin's MCP handler with a JSON-RPC request
    pub fn handle_request(&mut self, request: &Value) -> anyhow::Result<Value> {
        let input = serde_json::to_string(request)?;
        let output = self.plugin.call::<&str, &str>("mcp_handle", &input)?;
        let response: Value = serde_json::from_str(output)?;
        Ok(response)
    }

    /// Initialize the plugin (call `initialize` method)
    pub fn initialize(&mut self, capabilities: &Value) -> anyhow::Result<Value> {
        let input = serde_json::to_string(capabilities)?;
        let output = self.plugin.call::<&str, &str>("mcp_initialize", &input)?;
        let response: Value = serde_json::from_str(output)?;
        Ok(response)
    }

    pub fn name(&self) -> &str {
        &self.name
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
}
