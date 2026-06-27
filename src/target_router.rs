//! Target Router — selects upstream provider based on request properties
//! and configured target templates.
//!
//! Matching priority:
//!   1. `x-provider` header (explicit override)
//!   2. Model name in request body (matches against target.models)
//!   3. `default_provider` in config
//!   4. First configured target
//!   5. Single upstream URL (legacy fallback)

use crate::config::TargetConfig;

/// Resolve an upstream URL from the request context and target list.
pub fn resolve_upstream(
    targets: &[TargetConfig],
    default_provider: Option<&str>,
    provider_header: Option<&str>,
    request_body: Option<&serde_json::Value>,
) -> ResolvedTarget {
    // 1. Explicit header override
    if let Some(header) = provider_header {
        if let Some(t) = targets
            .iter()
            .find(|t| t.name == header || t.provider == header)
        {
            return ResolvedTarget::Found(t.clone());
        }
    }

    // 2. Model-based routing
    if let Some(body) = request_body {
        if let Some(model) = body.get("model").and_then(|m| m.as_str()) {
            for t in targets {
                if t.models
                    .iter()
                    .any(|m| model.starts_with(m) || model.contains(m.as_str()))
                {
                    return ResolvedTarget::Found(t.clone());
                }
            }
        }
    }

    // 3. Default provider
    if let Some(provider) = default_provider {
        if let Some(t) = targets
            .iter()
            .find(|t| t.name == provider || t.provider == provider)
        {
            return ResolvedTarget::Found(t.clone());
        }
    }

    // 4. First configured target
    if let Some(t) = targets.first() {
        return ResolvedTarget::Found(t.clone());
    }

    ResolvedTarget::NotFound
}

#[derive(Debug)]
pub enum ResolvedTarget {
    Found(TargetConfig),
    NotFound,
}

impl ResolvedTarget {
    pub fn base_url(&self) -> Option<&str> {
        match self {
            ResolvedTarget::Found(t) => Some(&t.base_url),
            ResolvedTarget::NotFound => None,
        }
    }

    pub fn provider(&self) -> Option<&str> {
        match self {
            ResolvedTarget::Found(t) => Some(&t.provider),
            ResolvedTarget::NotFound => None,
        }
    }
}

/// Provider-specific path mappings (e.g. Anthropic uses /v1/messages, Ollama uses /api/chat)
pub fn provider_path(provider: &str, request_path: &str) -> String {
    match provider {
        "anthropic" => {
            if request_path.contains("chat/completions") {
                request_path.replace("chat/completions", "messages")
            } else {
                request_path.to_string()
            }
        }
        "google" => {
            if request_path.contains("chat/completions") {
                "/v1beta/models/gemini-2.5-flash:generateContent".to_string()
            } else {
                request_path.to_string()
            }
        }
        "ollama" => {
            // Ollama uses /api/chat for chat, /api/generate for single prompts
            if request_path.contains("chat/completions") {
                "/api/chat".to_string()
            } else if request_path.contains("completions") {
                "/api/generate".to_string()
            } else {
                request_path.to_string()
            }
        }
        _ => request_path.to_string(), // OpenAI-compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_targets() -> Vec<TargetConfig> {
        vec![
            TargetConfig {
                name: "anthropic-fast".into(),
                provider: "anthropic".into(),
                base_url: "https://api.anthropic.com/v1".into(),
                models: vec!["claude-sonnet-4".into(), "claude-haiku-3".into()],
                ..Default::default()
            },
            TargetConfig {
                name: "openai-gpt5".into(),
                provider: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                models: vec!["gpt-5".into(), "gpt-4".into()],
                ..Default::default()
            },
        ]
    }

    #[test]
    fn test_header_override() {
        let t = sample_targets();
        let r = resolve_upstream(&t, None, Some("anthropic-fast"), None);
        assert_eq!(r.base_url(), Some("https://api.anthropic.com/v1"));
    }

    #[test]
    fn test_model_match() {
        let t = sample_targets();
        let body = serde_json::json!({"model": "gpt-5.4"});
        let r = resolve_upstream(&t, None, None, Some(&body));
        assert_eq!(r.base_url(), Some("https://api.openai.com/v1"));
    }

    #[test]
    fn test_default_provider() {
        let t = sample_targets();
        let r = resolve_upstream(&t, Some("anthropic"), None, None);
        assert_eq!(r.base_url(), Some("https://api.anthropic.com/v1"));
    }

    #[test]
    fn test_provider_path_anthropic() {
        assert_eq!(
            provider_path("anthropic", "/v1/chat/completions"),
            "/v1/messages"
        );
    }

    #[test]
    fn test_provider_path_openai() {
        assert_eq!(
            provider_path("openai", "/v1/chat/completions"),
            "/v1/chat/completions"
        );
    }

    #[test]
    fn test_provider_path_google() {
        assert_eq!(
            provider_path("google", "/v1/chat/completions"),
            "/v1beta/models/gemini-2.5-flash:generateContent"
        );
    }

    #[test]
    fn test_provider_path_fallback() {
        let paths = ["/v1/chat/completions", "/v1/messages", "/cdn/foo"];
        for p in &paths {
            let result = provider_path("unknown-provider", p);
            assert_eq!(result, *p, "unknown provider should passthrough path");
        }
    }

    #[test]
    fn test_resolve_not_found_with_empty_targets() {
        let result = resolve_upstream(&[], None, None, None);
        assert!(matches!(result, ResolvedTarget::NotFound));
        assert!(result.base_url().is_none());
        assert!(result.provider().is_none());
    }

    #[test]
    fn test_resolve_fallback_to_first_target() {
        let targets = vec![crate::config::TargetConfig {
            name: "default".into(),
            provider: "openai".into(),
            base_url: "https://default.example.com".into(),
            ..Default::default()
        }];
        let result = resolve_upstream(&targets, None, None, None);
        assert_eq!(result.base_url(), Some("https://default.example.com"));
    }

    #[test]
    fn test_resolve_by_header() {
        let targets = vec![crate::config::TargetConfig {
            name: "my-target".into(),
            provider: "anthropic".into(),
            base_url: "https://my.anthropic.com".into(),
            ..Default::default()
        }];
        let result = resolve_upstream(&targets, None, Some("my-target"), None);
        assert_eq!(result.base_url(), Some("https://my.anthropic.com"));
        assert_eq!(result.provider(), Some("anthropic"));
    }

    #[test]
    fn test_resolve_by_partial_model_match() {
        let targets = vec![crate::config::TargetConfig {
            name: "openai-box".into(),
            provider: "openai".into(),
            base_url: "https://openai.example.com".into(),
            models: vec!["gpt-4".into()],
            ..Default::default()
        }];
        let body = serde_json::json!({"model": "gpt-4-turbo"});
        let result = resolve_upstream(&targets, None, None, Some(&body));
        assert_eq!(result.base_url(), Some("https://openai.example.com"));
    }
}
