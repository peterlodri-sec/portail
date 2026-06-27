//! Plugin hook integration — calls loaded .vaked plugin hooks
//! from the proxy pipeline.
//!
//! Each request walks the loaded plugins and calls matching hooks.
//! Hooks return HookResult which can modify the body, add headers,
//! or abort the request.

use portail_plugin_sdk::{HookContext, HookPoint, HookResult};
use portail_vaked::{LoadedPlugin, PluginRegistry};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Call all plugins that handle the given hook point.
/// Returns the final modified HookResult (after all plugins).
pub fn call_plugin_hooks(
    registry: &Arc<Mutex<PluginRegistry>>,
    hook: HookPoint,
    request_id: &str,
    provider: Option<&str>,
    model: Option<&str>,
    body: Option<serde_json::Value>,
    headers: HashMap<String, String>,
) -> HookResult {
    let registry = match registry.lock() {
        Ok(r) => r,
        Err(_) => return HookResult::pass_through(),
    };

    let hook_str = match hook {
        HookPoint::PreRequest => "pre_request",
        HookPoint::PostResponse => "post_response",
        HookPoint::PreHookInject => "pre_hook_inject",
        HookPoint::PostHookInject => "post_hook_inject",
        HookPoint::OnError => "on_error",
        HookPoint::OnAuth => "on_auth",
    };
    let request_id_str = request_id.to_string();
    let provider_str = provider.map(|s| s.to_string());
    let model_str = model.map(|s| s.to_string());

    let mut result = HookResult::pass_through();

    for name in registry.list() {
        // Check if this plugin is a .vaked native or WASM
        let plugin = match registry.get(name) {
            Some(p) => p,
            None => continue,
        };

        // Check if plugin declares this hook
        let plugin_hooks = plugin_hook_list(plugin);
        if !plugin_hooks.iter().any(|h| h == hook_str) {
            continue;
        }

        let ctx = HookContext {
            hook: hook.clone(),
            request_id: request_id_str.clone(),
            provider: provider_str.clone(),
            model: model_str.clone(),
            body: if result.body.is_some() {
                result.body.clone()
            } else {
                body.clone()
            },
            headers: if !result.headers.is_empty() {
                result.headers.clone()
            } else {
                headers.clone()
            },
        };

        // For native plugins, call directly
        if let LoadedPlugin::Native(p) = plugin {
            let r = p.handle_hook(ctx);
            if r.abort_status.is_some() {
                return r;
            }
            if r.body.is_some() {
                result.body = r.body;
            }
            result.headers.extend(r.headers);
        }
        // For WASM plugins, would call via extism — stub for now
    }

    result
}

fn plugin_hook_list(plugin: &LoadedPlugin) -> Vec<String> {
    match plugin {
        LoadedPlugin::Native(p) => p.manifest().hooks.clone(),
        LoadedPlugin::Vaked(v) => {
            v.hooks
                .as_ref()
                .map(|h| {
                    h.values()
                        .flat_map(|v| v.iter().cloned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
    }
}

/// Initialize plugin registry: scan the vaked dir and load all plugins
pub fn init_plugin_registry(vaked_dir: &std::path::Path) -> Arc<Mutex<PluginRegistry>> {
    let mut registry = PluginRegistry::new(vaked_dir.to_path_buf());
    let loaded = registry.scan_dir().unwrap_or_default();
    if !loaded.is_empty() {
        tracing::info!(plugins = ?loaded, "loaded .vaked plugins from {}", vaked_dir.display());
    }
    Arc::new(Mutex::new(registry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_plugins_empty_registry() {
        let registry = Arc::new(Mutex::new(PluginRegistry::new(
            std::path::PathBuf::from("/tmp/nonexistent"),
        )));
        let result = call_plugin_hooks(
            &registry,
            HookPoint::PreRequest,
            "test-id",
            None,
            None,
            None,
            HashMap::new(),
        );
        assert!(result.abort_status.is_none());
        assert!(result.body.is_none());
    }
}
