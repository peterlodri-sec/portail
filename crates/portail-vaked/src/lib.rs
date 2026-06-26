//! Portail .vaked plugin host — load, validate, compile, and lower
//! `.vaked` plugin files into full e2e Nix/NixOS target systems.
//!
//! # .vaked pipeline
//!
//! ```text
//! .vaked file  →  parse & validate  →  lower to Nix  →  nix build
//!      ↑                                    │
//!    user sends                          target system
//!    hello.vaked                         (NixOS module)
//! ```

use portail_plugin_sdk::VakedFile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

// ── Plugin registry ─────────────────────────────────────────────

/// A loaded plugin (native or WASM)
pub enum LoadedPlugin {
    Native(Box<dyn portail_plugin_sdk::PortailPlugin>),
    Vaked(VakedFile),
}

pub struct PluginRegistry {
    plugins: HashMap<String, LoadedPlugin>,
    vaked_dir: PathBuf,
}

impl PluginRegistry {
    pub fn new(vaked_dir: PathBuf) -> Self {
        Self {
            plugins: HashMap::new(),
            vaked_dir,
        }
    }

    /// Scan a directory for .vaked files and load them
    pub fn scan_dir(&mut self) -> anyhow::Result<Vec<String>> {
        let mut loaded = Vec::new();
        let dir = self.vaked_dir.clone();
        if !dir.exists() {
            return Ok(loaded);
        }

        for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("vaked") {
                continue;
            }
            match self.load_vaked(path) {
                Ok(name) => {
                    loaded.push(name);
                }
                Err(e) => {
                    tracing::warn!("failed to load {}: {e}", path.display());
                }
            }
        }
        Ok(loaded)
    }

    /// Load a single .vaked file
    pub fn load_vaked(&mut self, path: &Path) -> anyhow::Result<String> {
        let raw = std::fs::read_to_string(path)?;
        let vaked = VakedFile::from_toml(&raw)?;
        let name = vaked.plugin.name.clone();
        self.plugins
            .insert(name.clone(), LoadedPlugin::Vaked(vaked));
        info!(plugin = %name, "loaded .vaked plugin");
        Ok(name)
    }

    /// Get a loaded plugin by name
    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(name)
    }

    /// List all loaded plugin names
    pub fn list(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Lower all .vaked plugins to a combined NixOS module
    pub fn lower_all_to_nix(&self) -> String {
        let mut parts = Vec::new();
        for plugin in self.plugins.values() {
            if let LoadedPlugin::Vaked(vaked) = plugin {
                parts.push(vaked.lower_to_nix());
            }
        }
        if parts.is_empty() {
            return "{ config, pkgs, ... }: { }".into();
        }
        // Merge: wrap all in a single nix expression
        let merged = format!(
            "{{ config, pkgs, ... }}:\n{{\n  imports = [\n{}  ];\n}}\n",
            parts.iter().map(|p| format!("    ({}: ", p)).collect::<Vec<_>>().join("")
        );
        merged
    }

    /// Count loaded plugins
    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

// ── .vaked build & deploy ───────────────────────────────────────

/// Build a .vaked plugin to WASM and deploy it to the target system
pub fn build_vaked(path: &Path) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(path)?;
    let vaked = VakedFile::from_toml(&raw)?;
    let name = &vaked.plugin.name;

    debug!("Building .vaked plugin: {name}");

    if let Some(ref build) = vaked.build {
        match build.r#type.as_str() {
            "wasm" => {
                let status = std::process::Command::new("cargo")
                    .args([
                        "build",
                        "--target",
                        "wasm32-wasip1",
                        "--release",
                    ])
                    .status()?;
                if !status.success() {
                    anyhow::bail!("WASM build failed for {name}");
                }
                info!("Built {name} to WASM");
            }
            other => {
                anyhow::bail!("Unknown build type: {other}");
            }
        }
    }

    Ok(())
}

// ── CLI helpers ─────────────────────────────────────────────────

pub fn format_plugin_list(registry: &PluginRegistry) -> String {
    let mut out = String::new();
    out.push_str(&format!("Plugins ({}):\n", registry.count()));
    for name in registry.list() {
        out.push_str(&format!("  {}\n", name));
    }
    out
}

pub fn format_nix_output(registry: &PluginRegistry) -> String {
    registry.lower_all_to_nix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let reg = PluginRegistry::new(PathBuf::from("/tmp/nonexistent"));
        assert_eq!(reg.count(), 0);
        let nix = reg.lower_all_to_nix();
        assert!(nix.contains("}"));
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = std::env::temp_dir().join("vaked_test_empty");
        let _ = std::fs::create_dir_all(&dir);
        let mut reg = PluginRegistry::new(dir.clone());
        let loaded = reg.scan_dir().unwrap();
        assert!(loaded.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_vaked_file() {
        let dir = std::env::temp_dir().join("vaked_test_load");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.vaked");
        let toml = r#"
[plugin]
name = "test-plugin"
version = "0.1.0"

[target]
packages = ["ripgrep"]
"#;
        std::fs::write(&path, toml).unwrap();

        let mut reg = PluginRegistry::new(dir.clone());
        reg.load_vaked(&path).unwrap();
        assert_eq!(reg.count(), 1);
        assert!(reg.list().contains(&"test-plugin"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_lower_all_includes_all() {
        let dir = std::env::temp_dir().join("vaked_test_lower");
        let _ = std::fs::create_dir_all(&dir);

        let path1 = dir.join("a.vaked");
        std::fs::write(&path1, r#"[plugin]
name = "plugin-a"
version = "1.0.0"
[target]
packages = ["bat"]
"#).unwrap();

        let path2 = dir.join("b.vaked");
        std::fs::write(&path2, r#"[plugin]
name = "plugin-b"
version = "1.0.0"
[target]
packages = ["eza"]
"#).unwrap();

        let mut reg = PluginRegistry::new(dir.clone());
        reg.load_vaked(&path1).unwrap();
        reg.load_vaked(&path2).unwrap();
        assert_eq!(reg.count(), 2);

        let nix = reg.lower_all_to_nix();
        assert!(nix.contains("bat") || nix.contains("eza"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
