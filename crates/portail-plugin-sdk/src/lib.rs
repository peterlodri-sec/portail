//! Portail Plugin SDK — build WASM-compiled plugins for the Portail gateway.
//!
//! # Plugin lifecycle
//!
//! 1. **Init** — called once at plugin load. Return PluginConfig with
//!    declared hooks and capabilities.
//! 2. **Hook** — called per event. Receive a HookContext, return a
//!    HookResult to modify the pipeline or pass through.
//! 3. **Health** — called periodically. Return healthy/failing.
//! 4. **Shutdown** — called before plugin is unloaded.
//!
//! # Compile to WASM
//!
//! ```toml
//! [package]
//! name = "my-plugin"
//!
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! portail-plugin-sdk = "3.0"
//! extism-pdk = "1.0"
//! ```
//!
//! Build: `cargo build --target wasm32-wasip1 --release`

use serde::{Deserialize, Serialize};

// ── Plugin Identity ─────────────────────────────────────────────

/// Metadata declared by every plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub hooks: Vec<String>,
    pub capabilities: Vec<String>,
    pub target_system: Option<TargetSystem>,
}

/// Target system descriptor — lowered to Nix/NixOS config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSystem {
    pub nixos_module: bool,
    pub packages: Vec<String>,
    pub services: Vec<String>,
    pub env: Vec<String>,
}

// ── Hook System ─────────────────────────────────────────────────

/// All hook points in the Portail pipeline
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HookPoint {
    /// Before an upstream request is sent
    PreRequest,
    /// After an upstream response is received
    PostResponse,
    /// Before hook injection into messages
    PreHookInject,
    /// After hook injection
    PostHookInject,
    /// On request error
    OnError,
    /// On auth decision
    OnAuth,
}

/// Context passed to a hook handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    pub hook: HookPoint,
    pub request_id: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub body: Option<serde_json::Value>,
    pub headers: std::collections::HashMap<String, String>,
}

/// Result a hook returns to modify the pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// Modify the request body (None = pass through)
    pub body: Option<serde_json::Value>,
    /// Add/override headers
    pub headers: std::collections::HashMap<String, String>,
    /// Abort the request with this status (None = continue)
    pub abort_status: Option<u16>,
    /// Abort message
    pub abort_message: Option<String>,
}

impl HookResult {
    pub fn pass_through() -> Self {
        Self {
            body: None,
            headers: std::collections::HashMap::new(),
            abort_status: None,
            abort_message: None,
        }
    }

    pub fn abort(status: u16, message: &str) -> Self {
        Self {
            body: None,
            headers: std::collections::HashMap::new(),
            abort_status: Some(status),
            abort_message: Some(message.into()),
        }
    }

    pub fn with_body(body: serde_json::Value) -> Self {
        Self {
            body: Some(body),
            ..Self::pass_through()
        }
    }
}

// ── Plugin Trait (for native plugins) ───────────────────────────

/// A compiled Portail plugin. Implement this trait, compile to WASM.
///
/// # Example
///
/// ```ignore
/// use portail_plugin_sdk::*;
/// use extism_pdk::*;
///
/// #[plugin_fn]
/// pub fn init(Json(manifest): Json<PluginManifest>) -> FnResult<Json<PluginManifest>> {
///     Ok(Json(PluginManifest {
///         name: "hello".into(),
///         version: "1.0.0".into(),
///         description: "My first plugin".into(),
///         hooks: vec!["pre_request".into()],
///         capabilities: vec!["log".into()],
///         target_system: None,
///     }))
/// }
///
/// #[plugin_fn]
/// pub fn handle_hook(Json(ctx): Json<HookContext>) -> FnResult<Json<HookResult>> {
///     println!("hook: {:?}", ctx.hook);
///     Ok(Json(HookResult::pass_through()))
/// }
/// ```
pub trait PortailPlugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn handle_hook(&self, ctx: HookContext) -> HookResult;
    fn health(&self) -> bool;
    fn shutdown(&self);
}

// ── .vaked file format ──────────────────────────────────────────

/// A `.vaked` file — declarative plugin + target system descriptor.
/// Users send this file, Portail parses it into a full e2e Nix system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VakedFile {
    pub plugin: VakedPlugin,
    pub build: Option<VakedBuild>,
    pub hooks: Option<std::collections::HashMap<String, Vec<String>>>,
    pub capabilities: Option<VakedCapabilities>,
    pub target: Option<VakedTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VakedPlugin {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VakedBuild {
    pub r#type: String,
    pub entry: String,
    pub flags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VakedCapabilities {
    pub requires: Vec<String>,
    pub grants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VakedTarget {
    pub nixos_module: Option<bool>,
    pub packages: Option<Vec<String>>,
    pub services: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub docker: Option<VakedDocker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VakedDocker {
    pub image: String,
    pub volumes: Vec<String>,
    pub ports: Vec<String>,
}

impl VakedFile {
    /// Parse a .vaked TOML string into a VakedFile
    pub fn from_toml(input: &str) -> anyhow::Result<Self> {
        let vaked: VakedFile = toml::from_str(input)?;
        vaked.validate()?;
        Ok(vaked)
    }

    /// Validate the .vaked file has required fields
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.plugin.name.is_empty() {
            anyhow::bail!("plugin.name is required");
        }
        if self.plugin.version.is_empty() {
            anyhow::bail!("plugin.version is required");
        }
        Ok(())
    }

    /// Lower the .vaked file to a list of Nix packages / NixOS modules
    pub fn lower_to_nix(&self) -> String {
        let mut nix = String::new();
        nix.push_str("{ config, pkgs, ... }:\n{\n");

        if let Some(ref target) = self.target {
            if let Some(ref pkgs) = target.packages {
                if !pkgs.is_empty() {
                    nix.push_str("  environment.systemPackages = with pkgs; [\n");
                    for pkg in pkgs {
                        nix.push_str(&format!("    {}\n", pkg));
                    }
                    nix.push_str("  ];\n");
                }
            }
            if let Some(ref services) = target.services {
                for svc in services {
                    nix.push_str(&format!(
                        "  services.{} = {{\n    enable = true;\n  }};\n",
                        svc
                    ));
                }
            }
            if let Some(ref env) = target.env {
                for e in env {
                    nix.push_str(&format!("  environment.variables.\"{}\" = \"\";\n", e));
                }
            }
            if let Some(ref docker) = target.docker {
                nix.push_str(&format!(
                    "  virtualisation.docker.containers.{} = {{\n    image = \"{}\";\n  }};\n",
                    self.plugin.name, docker.image
                ));
            }
        }

        nix.push_str("}\n");
        nix
    }

    /// Lower to a Nix flake snippet
    pub fn lower_to_flake(&self) -> String {
        format!(
            r#"# Auto-generated from {name}.vaked
{name} = {{
  plugin = {{
    name = "{name}";
    version = "{version}";
  }};
  hooks = {hooks};
}};
"#,
            name = self.plugin.name,
            version = self.plugin.version,
            hooks = serde_json::to_string_pretty(&self.hooks).unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vaked_parse_minimal() {
        let toml = r#"
[plugin]
name = "hello-world"
version = "1.0.0"
"#;
        let vaked = VakedFile::from_toml(toml).unwrap();
        assert_eq!(vaked.plugin.name, "hello-world");
        assert_eq!(vaked.plugin.version, "1.0.0");
    }

    #[test]
    fn test_vaked_parse_full() {
        let toml = r#"
[plugin]
name = "auth-enforcer"
version = "0.1.0"
description = "Enforce auth headers on all requests"
language = "rust"

[build]
type = "wasm"
entry = "src/lib.rs"

[hooks]
pre_request = ["check_auth"]
on_auth = ["enforce"]

[capabilities]
requires = ["http"]
grants = ["auth:read"]

[target]
nixos_module = true
packages = ["portail", "curl"]
services = ["portail"]
env = ["AUTH_TOKEN"]
"#;
        let vaked = VakedFile::from_toml(toml).unwrap();
        assert_eq!(vaked.plugin.name, "auth-enforcer");
        assert!(vaked.build.is_some());
        assert!(vaked.target.is_some());
        let nix = vaked.lower_to_nix();
        assert!(nix.contains("portail"));
        assert!(nix.contains("curl"));
    }

    #[test]
    fn test_vaked_validation_fails_without_name() {
        let toml = r#"
[plugin]
version = "1.0.0"
"#;
        let result = VakedFile::from_toml(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_lower_to_nix_with_docker() {
        let toml = r#"
[plugin]
name = "redis-cache"
version = "2.0.0"

[target]
packages = ["redis"]

[target.docker]
image = "redis:7"
volumes = ["/data:/data"]
ports = ["6379:6379"]
"#;
        let vaked = VakedFile::from_toml(toml).unwrap();
        let nix = vaked.lower_to_nix();
        assert!(nix.contains("redis"));
        assert!(nix.contains("docker.containers"));
    }

    #[test]
    fn test_manifest_roundtrip() {
        let m = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "test plugin".into(),
            hooks: vec!["pre_request".into()],
            capabilities: vec!["log".into()],
            target_system: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test");
    }
}
