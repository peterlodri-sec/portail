//! Request Logger Plugin — first official Portail .vaked plugin.
//!
//! Hooks into pre_request to log all upstream requests.
//! Hooks into post_response to log response status.
//!
//! Compile:
//!   cargo build --target wasm32-wasip1 --release
//!
//! Deploy:
//!   portail vaked load examples/plugins/request-logger/plugin.vaked
//!   portail vaked lower examples/plugins/request-logger/plugin.vaked

use extism_pdk::*;
use portail_plugin_sdk::*;
use serde_json::json;

const PLUGIN_NAME: &str = "request-logger";
const PLUGIN_VERSION: &str = "1.0.0";

// ── Plugin Init ─────────────────────────────────────────────────

#[plugin_fn]
pub fn init() -> FnResult<Json<PluginManifest>> {
    Ok(Json(PluginManifest {
        name: PLUGIN_NAME.into(),
        version: PLUGIN_VERSION.into(),
        description: "Logs all upstream requests and responses".into(),
        hooks: vec![
            "pre_request".into(),
            "post_response".into(),
            "on_error".into(),
        ],
        capabilities: vec!["log".into(), "observe".into()],
        target_system: Some(TargetSystem {
            nixos_module: false,
            packages: vec![],
            services: vec![],
            env: vec!["LOG_LEVEL=info".into()],
        }),
    }))
}

// ── Hook Handlers ────────────────────────────────────────────────

#[plugin_fn]
pub fn handle_hook(Json(ctx): Json<HookContext>) -> FnResult<Json<HookResult>> {
    match ctx.hook {
        HookPoint::PreRequest => {
            let model = ctx.model.as_deref().unwrap_or("unknown");
            let provider = ctx.provider.as_deref().unwrap_or("unknown");
            let body_preview = ctx.body.as_ref()
                .and_then(|b| b.get("messages"))
                .and_then(|m| m.as_array())
                .map(|a| format!("{} messages", a.len()))
                .unwrap_or_else(|| "no messages".into());

            set_var!("log", &format!(
                "[request-logger] pre_request: provider={provider} model={model} {body_preview}"
            )).ok();

            // Pass through — just observing
            Ok(Json(HookResult::pass_through()))
        }
        HookPoint::PostResponse => {
            set_var!("log", "[request-logger] post_response: response received").ok();
            Ok(Json(HookResult::pass_through()))
        }
        HookPoint::OnError => {
            set_var!("log", "[request-logger] on_error: request failed").ok();
            Ok(Json(HookResult::abort(502, "upstream error (logged)")))
        }
        _ => Ok(Json(HookResult::pass_through())),
    }
}

// ── Health ───────────────────────────────────────────────────────

#[plugin_fn]
pub fn health() -> FnResult<Json<serde_json::Value>> {
    Ok(Json(json!({
        "status": "healthy",
        "name": PLUGIN_NAME,
        "version": PLUGIN_VERSION,
    })))
}
