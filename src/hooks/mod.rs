use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InjectMode {
    Prepend,
    Append,
}

impl Default for InjectMode {
    fn default() -> Self {
        Self::Prepend
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub id: String,
    pub match_agent: Option<String>,
    pub match_path: Option<String>,
    pub match_event_type: Option<String>,
    #[serde(default)]
    pub inject: InjectMode,
    pub content: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
}

pub struct HookStore {
    hooks: RwLock<Vec<Hook>>,
}

impl Default for HookStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HookStore {
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(Vec::new()),
        }
    }

    pub fn add(&self, mut hook: Hook) -> Hook {
        if hook.id.is_empty() {
            hook.id = uuid::Uuid::new_v4().to_string();
        }
        self.hooks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(hook.clone());
        hook
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut hooks = self.hooks.write().unwrap_or_else(|e| e.into_inner());
        let pos = hooks.iter().position(|h| h.id == id);
        if let Some(p) = pos {
            hooks.remove(p);
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> Vec<Hook> {
        self.hooks.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn get(&self, id: &str) -> Option<Hook> {
        self.hooks
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|h| h.id == id)
            .cloned()
    }

    pub fn toggle(&self, id: &str) -> Option<Hook> {
        let mut hooks = self.hooks.write().unwrap_or_else(|e| e.into_inner());
        if let Some(hook) = hooks.iter_mut().find(|h| h.id == id) {
            hook.enabled = !hook.enabled;
            Some(hook.clone())
        } else {
            None
        }
    }

    pub fn clear(&self) {
        self.hooks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub fn match_message(&self, path: &str) -> Vec<Hook> {
        let hooks = self.hooks.read().unwrap_or_else(|e| e.into_inner());
        let mut matched: Vec<Hook> = hooks
            .iter()
            .filter(|h| h.enabled)
            .filter(|h| {
                h.match_event_type.is_none()
                    && match &h.match_path {
                        Some(p) => path.starts_with(p.as_str()) || p == "*",
                        None => true,
                    }
            })
            .cloned()
            .collect();
        matched.sort_by_key(|h| h.priority);
        matched
    }

    pub fn match_event(&self, agent_id: &str, event_type: &str) -> Vec<Hook> {
        let hooks = self.hooks.read().unwrap_or_else(|e| e.into_inner());
        let mut matched: Vec<Hook> = hooks
            .iter()
            .filter(|h| h.enabled)
            .filter(|h| {
                (match &h.match_agent {
                    Some(a) => agent_id.contains(a.as_str()) || a == "*",
                    None => true,
                }) && (match &h.match_event_type {
                    Some(t) => event_type.contains(t.as_str()) || t == "*",
                    None => true,
                })
            })
            .cloned()
            .collect();
        matched.sort_by_key(|h| h.priority);
        matched
    }
}

/// Inject hooks into a JSON message body (e.g. chat completion request).
pub fn apply_message_hooks(body: &serde_json::Value, hooks: &[Hook]) -> Option<serde_json::Value> {
    if hooks.is_empty() {
        return None;
    }
    let mut body = body.clone();
    let messages = body.get_mut("messages")?.as_array_mut()?;

    for hook in hooks {
        let msg = serde_json::json!({
            "role": "system",
            "content": hook.content,
        });
        match hook.inject {
            InjectMode::Prepend => messages.insert(0, msg),
            InjectMode::Append => messages.push(msg),
        }
    }
    Some(body)
}

/// Apply event hooks and join with newlines.
pub fn apply_event_hooks(_body: &serde_json::Value, hooks: &[Hook]) -> Option<String> {
    if hooks.is_empty() {
        return None;
    }
    let parts: Vec<String> = hooks.iter().map(|h| h.content.clone()).collect();
    Some(parts.join("\n"))
}

// ── HTTP handlers ─────────────────────────────────────────────────────

pub async fn handle_list(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> axum::Json<Vec<Hook>> {
    axum::Json(state.hooks.list())
}

pub async fn handle_create(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::Json(hook): axum::Json<Hook>,
) -> (axum::http::StatusCode, axum::Json<Hook>) {
    let created = state.hooks.add(hook);
    (StatusCode::CREATED, axum::Json(created))
}

pub async fn handle_get(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    match state.hooks.get(&id) {
        Some(hook) => axum::Json(hook).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn handle_toggle(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    match state.hooks.toggle(&id) {
        Some(hook) => axum::Json(hook).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn handle_delete(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    if state.hooks.remove(&id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hook(id: &str, agent: Option<&str>, path: Option<&str>, et: Option<&str>) -> Hook {
        Hook {
            id: id.into(),
            match_agent: agent.map(String::from),
            match_path: path.map(String::from),
            match_event_type: et.map(String::from),
            inject: InjectMode::Prepend,
            content: "injected".into(),
            enabled: true,
            priority: 0,
        }
    }

    #[test]
    fn store_add_list_remove() {
        let store = HookStore::new();
        store.add(test_hook("h1", None, None, None));
        store.add(test_hook("h2", None, None, None));
        assert_eq!(store.list().len(), 2);
        assert!(store.remove("h1"));
        assert_eq!(store.list().len(), 1);
        assert!(!store.remove("nonexistent"));
    }

    #[test]
    fn store_get() {
        let store = HookStore::new();
        store.add(test_hook("h1", None, None, None));
        assert!(store.get("h1").is_some());
        assert!(store.get("nope").is_none());
    }

    #[test]
    fn store_toggle() {
        let store = HookStore::new();
        store.add(test_hook("h1", None, None, None));
        let toggled = store.toggle("h1").unwrap();
        assert!(!toggled.enabled);
        let toggled = store.toggle("h1").unwrap();
        assert!(toggled.enabled);
        assert!(store.toggle("nope").is_none());
    }

    #[test]
    fn store_clear() {
        let store = HookStore::new();
        store.add(test_hook("h1", None, None, None));
        store.add(test_hook("h2", None, None, None));
        store.clear();
        assert!(store.list().is_empty());
    }

    #[test]
    fn match_message_by_path() {
        let store = HookStore::new();
        store.add(test_hook("h1", None, Some("/v1/chat"), None));
        store.add(test_hook("h2", None, Some("/v1/embed"), None));

        assert_eq!(store.match_message("/v1/chat/completions").len(), 1);
        assert_eq!(store.match_message("/v1/embeddings").len(), 1);
        assert_eq!(store.match_message("/other").len(), 0);
    }

    #[test]
    fn match_message_wildcard() {
        let store = HookStore::new();
        store.add(test_hook("h1", None, Some("*"), None));
        assert_eq!(store.match_message("/anything").len(), 1);
    }

    #[test]
    fn match_event_by_agent_and_type() {
        let store = HookStore::new();
        store.add(test_hook("h1", Some("my-agent"), None, Some("started")));
        assert_eq!(store.match_event("my-agent", "started").len(), 1);
        assert_eq!(store.match_event("other-agent", "started").len(), 0);
        assert_eq!(store.match_event("my-agent", "stopped").len(), 0);
    }

    #[test]
    fn match_message_sorted_by_priority() {
        let store = HookStore::new();
        let mut h1 = test_hook("h1", None, Some("/v1"), None);
        h1.priority = 10;
        let mut h2 = test_hook("h2", None, Some("/v1"), None);
        h2.priority = 1;
        store.add(h1);
        store.add(h2);
        let matched = store.match_message("/v1/chat");
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].id, "h2");
        assert_eq!(matched[1].id, "h1");
    }

    #[test]
    fn apply_message_hooks_prepends_system() {
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "hi"}]
        });
        let hooks = vec![test_hook("h1", None, None, None)];
        let result = apply_message_hooks(&body, &hooks).unwrap();
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
    }

    #[test]
    fn serde_defaults() {
        let json = r#"{"id":"x","content":"c","enabled":true}"#;
        let hook: Hook = serde_json::from_str(json).unwrap();
        assert!(hook.enabled);
        assert_eq!(hook.inject, InjectMode::Prepend);
        assert_eq!(hook.priority, 0);
    }

    #[test]
    fn serde_enabled_false() {
        let json = r#"{"id":"x","content":"c","enabled":false}"#;
        let hook: Hook = serde_json::from_str(json).unwrap();
        assert!(!hook.enabled);
    }

    #[test]
    fn inject_mode_default_is_prepend() {
        assert_eq!(InjectMode::default(), InjectMode::Prepend);
    }

    #[test]
    fn poisoned_lock_recovery() {
        use std::sync::Arc;
        let store = Arc::new(HookStore::new());
        // Poison the lock
        let store_clone = store.clone();
        let _ = std::thread::spawn(move || {
            let _guard = store_clone.hooks.write().unwrap();
            panic!("poison");
        })
        .join();
        // Should still work via unwrap_or_else
        store.add(test_hook("recovered", None, None, None));
        assert_eq!(store.list().len(), 1);
    }
}
