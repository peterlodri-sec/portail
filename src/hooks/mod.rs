use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub id: String,
    pub match_agent: Option<String>,
    pub match_path: Option<String>,
    pub match_event_type: Option<String>,
    pub inject: InjectMode,
    pub content: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectMode {
    Prepend,
    Append,
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

    pub fn add(&self, mut hook: Hook) {
        if hook.id.is_empty() {
            hook.id = uuid::Uuid::new_v4().to_string();
        }
        self.hooks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(hook);
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

    pub fn match_message(&self, path: &str) -> Vec<Hook> {
        let hooks = self.hooks.read().unwrap_or_else(|e| e.into_inner());
        hooks
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
            .collect()
    }

    pub fn match_event(&self, agent_id: &str, event_type: &str) -> Vec<Hook> {
        let hooks = self.hooks.read().unwrap_or_else(|e| e.into_inner());
        hooks
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
            .collect()
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

// ── HTTP handlers ─────────────────────────────────────────────────────

pub async fn handle_list(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> axum::Json<Vec<Hook>> {
    axum::Json(state.hooks.list())
}

pub async fn handle_create(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::Json(hook): axum::Json<Hook>,
) -> impl axum::response::IntoResponse {
    state.hooks.add(hook);
    (axum::http::StatusCode::CREATED, "created")
}

pub async fn handle_delete(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    if state.hooks.remove(&id) {
        (axum::http::StatusCode::NO_CONTENT, "")
    } else {
        (axum::http::StatusCode::NOT_FOUND, "not found")
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
    fn apply_message_hooks_prepend() {
        let body = serde_json::json!({"messages": [{"role": "user", "content": "hello"}]});
        let hooks = vec![Hook {
            id: "h1".into(),
            match_agent: None,
            match_path: None,
            match_event_type: None,
            inject: InjectMode::Prepend,
            content: "be nice".into(),
            enabled: true,
        }];
        let modified = apply_message_hooks(&body, &hooks).unwrap();
        let msgs = modified["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["content"], "be nice");
        assert_eq!(msgs[1]["content"], "hello");
    }

    #[test]
    fn apply_message_hooks_append() {
        let body = serde_json::json!({"messages": [{"role": "user", "content": "hello"}]});
        let hooks = vec![Hook {
            id: "h1".into(),
            match_agent: None,
            match_path: None,
            match_event_type: None,
            inject: InjectMode::Append,
            content: "sign off".into(),
            enabled: true,
        }];
        let modified = apply_message_hooks(&body, &hooks).unwrap();
        let msgs = modified["messages"].as_array().unwrap();
        assert_eq!(msgs[1]["content"], "sign off");
    }

    #[test]
    fn disabled_hook_not_matched() {
        let store = HookStore::new();
        let mut hook = test_hook("h1", None, Some("*"), None);
        hook.enabled = false;
        store.add(hook);
        assert!(store.match_message("/anything").is_empty());
    }
}
