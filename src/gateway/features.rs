//! Feature Virtualizer — emulates provider features that aren't natively supported.
//!
//! # The Problem
//!
//! Different AI providers support different feature sets:
//!
//! | Feature           | OpenAI  | DeepSeek | Anthropic | Google   | Ollama   |
//! |-------------------|---------|----------|-----------|----------|----------|
//! | tool_choice       | native  | native   | native    | native   | ❌       |
//! | response_format   | native  | native   | ❌        | ❌       | ❌       |
//! | seed              | native  | ❌       | ❌        | ❌       | native   |
//! | frequency_penalty | native  | native   | ❌        | ❌       | native   |
//! | presence_penalty  | native  | native   | ❌        | ❌       | native   |
//! | prefix_completion | ❌      | native   | ❌        | ❌       | ❌       |
//!
//! # Solution
//!
//! The virtualizer sits between the client and the schema adapter.
//! For each feature that a provider doesn't natively support, it applies
//! a fallback strategy. The client always sees a consistent OpenAI-format
//! response regardless of the provider's actual capabilities.
//!
//! # Fallback Strategies
//!
//! - **Strip + Warn**: Remove unsupported params, log a warning.
//! - **Prompt Inject**: Inject instructions into the system message to
//!   simulate the feature (e.g., "Respond in JSON format" for `response_format`).
//! - **Response Transform**: Post-process the response to emulate the feature
//!   (e.g., detect tool calls in plain text and restructure as `tool_calls`).

use serde_json::{Value, json};
use tracing::warn;

// ── Feature Capability Matrix ───────────────────────────────────────

/// What a provider supports for a specific feature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Support {
    /// Feature works natively — pass through as-is.
    Native,
    /// Feature not supported. Strip it and apply the configured fallback.
    Fallback(FallbackStrategy),
}

/// Fallback strategy when a feature isn't natively supported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FallbackStrategy {
    /// Remove the field silently.
    Strip,
    /// Remove the field and log a warning.
    StripWarn,
    /// Emulate by injecting instructions into the system prompt.
    PromptInject,
    /// Emulate by processing the response for expected patterns.
    ResponseTransform,
}

/// Get the capability matrix for a given provider.
/// Returns a list of (feature_name, support) pairs.
pub fn capabilities(provider: &str) -> Vec<(&'static str, Support)> {
    match provider {
        "openai" => vec![
            ("tool_choice", Support::Native),
            ("response_format", Support::Native),
            ("seed", Support::Native),
            ("frequency_penalty", Support::Native),
            ("presence_penalty", Support::Native),
            ("logprobs", Support::Native),
            ("stream", Support::Native),
            ("prefix", Support::Fallback(FallbackStrategy::StripWarn)),
        ],
        "deepseek" => vec![
            ("tool_choice", Support::Native),
            ("response_format", Support::Native),
            ("seed", Support::Fallback(FallbackStrategy::StripWarn)),
            ("frequency_penalty", Support::Native),
            ("presence_penalty", Support::Native),
            ("logprobs", Support::Fallback(FallbackStrategy::StripWarn)),
            ("stream", Support::Native),
            ("prefix", Support::Native),
        ],
        "anthropic" => vec![
            ("tool_choice", Support::Native),
            (
                "response_format",
                Support::Fallback(FallbackStrategy::PromptInject),
            ),
            ("seed", Support::Fallback(FallbackStrategy::StripWarn)),
            (
                "frequency_penalty",
                Support::Fallback(FallbackStrategy::StripWarn),
            ),
            (
                "presence_penalty",
                Support::Fallback(FallbackStrategy::StripWarn),
            ),
            ("logprobs", Support::Fallback(FallbackStrategy::StripWarn)),
            ("stream", Support::Native),
            ("prefix", Support::Fallback(FallbackStrategy::StripWarn)),
        ],
        "google" => vec![
            ("tool_choice", Support::Native),
            (
                "response_format",
                Support::Fallback(FallbackStrategy::PromptInject),
            ),
            ("seed", Support::Fallback(FallbackStrategy::StripWarn)),
            (
                "frequency_penalty",
                Support::Fallback(FallbackStrategy::StripWarn),
            ),
            (
                "presence_penalty",
                Support::Fallback(FallbackStrategy::StripWarn),
            ),
            ("logprobs", Support::Fallback(FallbackStrategy::StripWarn)),
            ("stream", Support::Native),
            ("prefix", Support::Fallback(FallbackStrategy::StripWarn)),
        ],
        "ollama" => vec![
            (
                "tool_choice",
                Support::Fallback(FallbackStrategy::ResponseTransform),
            ),
            (
                "response_format",
                Support::Fallback(FallbackStrategy::PromptInject),
            ),
            ("seed", Support::Native),
            ("frequency_penalty", Support::Native),
            ("presence_penalty", Support::Native),
            ("logprobs", Support::Fallback(FallbackStrategy::StripWarn)),
            ("stream", Support::Native),
            ("prefix", Support::Fallback(FallbackStrategy::StripWarn)),
        ],
        _ => vec![], // Unknown provider → no capabilities declared
    }
}

// ── Request Virtualization ──────────────────────────────────────────

/// Virtualize a request: apply fallback strategies for unsupported features.
///
/// This is called BEFORE `ProviderAdapter::adapt_request()`. It:
/// 1. Checks each feature in the request against the provider's capabilities
/// 2. For unsupported features, applies the fallback (strip, prompt-inject, etc.)
/// 3. Returns a list of warnings for the operator
pub fn virtualize_request(provider: &str, body: &mut Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let caps = capabilities(provider);

    for (feature_name, support) in &caps {
        match support {
            Support::Native => {
                // Feature is supported — do nothing, let it pass through to schema adapter
            }
            Support::Fallback(strategy) => {
                if !body.get(*feature_name).is_some_and(|v| !v.is_null()) {
                    continue; // feature not requested, skip
                }
                match strategy {
                    FallbackStrategy::Strip => {
                        body.as_object_mut().map(|m| m.remove(*feature_name));
                    }
                    FallbackStrategy::StripWarn => {
                        let val = body.as_object_mut().and_then(|m| m.remove(*feature_name));
                        if let Some(v) = val {
                            warnings.push(format!(
                                "{}: '{}' is not supported by '{}', stripped (value: {})",
                                provider, feature_name, provider, v
                            ));
                            warn!(
                                feature = %feature_name,
                                provider = %provider,
                                "unsupported feature stripped"
                            );
                        }
                    }
                    FallbackStrategy::PromptInject => {
                        if body
                            .as_object()
                            .and_then(|m| m.get(*feature_name))
                            .is_some()
                        {
                            let injected = inject_fallback_prompt(body, feature_name);
                            body.as_object_mut().map(|m| m.remove(*feature_name));
                            if let Some(msg) = injected {
                                warnings.push(msg);
                            }
                        }
                    }
                    FallbackStrategy::ResponseTransform => {
                        // Just strip for now — transform happens in response path
                        body.as_object_mut().map(|m| m.remove(*feature_name));
                        warnings.push(format!(
                            "{}: '{}' emulated via response transform",
                            provider, feature_name
                        ));
                    }
                }
            }
        }
    }

    warnings
}

/// Inject a system prompt instruction to simulate a missing feature.
fn inject_fallback_prompt(body: &mut Value, feature: &str) -> Option<String> {
    let instruction = match feature {
        "response_format" => {
            // The request has response_format. Check what type.
            let format_type = body
                .as_object()
                .and_then(|m| m.get("response_format"))
                .and_then(|rf| rf.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("text");

            if format_type == "json_object" || format_type == "json_schema" {
                Some(
                    "You MUST respond in valid JSON format only. \
                       Do NOT include any explanations, markdown formatting, \
                       or text outside the JSON object. \
                       Your entire response must be parseable as JSON."
                        .to_string(),
                )
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(instr) = instruction {
        append_to_system_prompt(body, &instr);
        Some(format!(
            "{}: '{feature}' emulated via system prompt injection",
            body.get("model").and_then(|m| m.as_str()).unwrap_or("?"),
        ))
    } else {
        None
    }
}

/// Append text to the system message, creating one if needed.
fn append_to_system_prompt(body: &mut Value, text: &str) {
    // Check if messages array exists
    if body.get("messages").and_then(|m| m.as_array()).is_none() {
        if let Some(m) = body.as_object_mut() {
            m.insert(
                "messages".into(),
                json!([{"role": "system", "content": text}]),
            );
        }
        return;
    }

    let messages = body
        .as_object_mut()
        .and_then(|m| m.get_mut("messages"))
        .and_then(|m| m.as_array_mut())
        .expect("messages array exists after check above");

    // Try to find existing system message
    if let Some(sys_msg) = messages
        .iter_mut()
        .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("system"))
    {
        if let Some(existing) = sys_msg.get_mut("content") {
            if let Some(s) = existing.as_str() {
                *existing = json!(format!("{}\n\n{}", s, text));
            } else if let Some(arr) = existing.as_array() {
                let mut arr = arr.clone();
                arr.push(json!({"type": "text", "text": text}));
                *existing = json!(arr);
            }
            return;
        }
    }

    // No system message → insert one at position 0
    messages.insert(0, json!({"role": "system", "content": text}));
}

// ── Response Virtualization ─────────────────────────────────────────

/// Virtualize a response: post-process to emulate features the provider doesn't natively support.
///
/// Called AFTER `ProviderAdapter::adapt_response()`. It:
/// 1. Checks what features were emulated
/// 2. Applies response transforms (e.g., detect tool calls in plain text)
pub fn virtualize_response(
    provider: &str,
    body: &mut Value,
    emulated_features: &[&str],
) -> Vec<String> {
    let mut warnings = Vec::new();

    for feature in emulated_features {
        match *feature {
            "tool_choice" => {
                // For providers without native tool calling (Ollama),
                // tools were described in the system prompt.
                // Check if the response matches a tool call pattern.
                if let Some(tool_call) = extract_tool_call_from_text(body) {
                    inject_tool_call(body, tool_call);
                    warnings.push(format!(
                        "{}: 'tool_choice' emulated — detected tool call in text response",
                        provider
                    ));
                }
            }
            "response_format" => {
                // Response came back as text, but we asked for JSON.
                // Validate it's parseable JSON.
                let content = body
                    .as_object()
                    .and_then(|m| m.get("choices"))
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|c| c.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str());

                if let Some(text) = content {
                    if serde_json::from_str::<Value>(text).is_err() {
                        warnings.push(format!(
                            "{}: 'response_format' emulated but response is not valid JSON",
                            provider
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    warnings
}

/// Extract a tool call from a plain-text assistant response.
/// Looks for JSON patterns like {"function": "name", "arguments": {...}}
/// in the assistant's reply.
fn extract_tool_call_from_text(body: &Value) -> Option<ToolCall> {
    let content = body
        .as_object()
        .and_then(|m| m.get("choices"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())?;

    // Try to find tool call JSON in the text
    // Pattern: a JSON object with "function"/"tool" + "arguments"/"args" keys
    // We scan for { ... } blocks in the text
    let mut depth = 0;
    let mut start = None;

    for (i, ch) in content.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        let segment = &content[s..=i];
                        if let Ok(val) = serde_json::from_str::<Value>(segment) {
                            // Check if this looks like a tool call
                            let has_func = val
                                .get("function")
                                .or(val.get("tool"))
                                .or(val.get("name"))
                                .and_then(|v| v.as_str())
                                .is_some();
                            let has_args = val
                                .get("arguments")
                                .or(val.get("args"))
                                .or(val.get("parameters"))
                                .is_some();

                            if has_func && has_args {
                                let name = val
                                    .get("function")
                                    .or(val.get("tool"))
                                    .or(val.get("name"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let args = val
                                    .get("arguments")
                                    .or(val.get("args"))
                                    .or(val.get("parameters"))
                                    .cloned()
                                    .unwrap_or(json!({}));
                                return Some(ToolCall {
                                    name,
                                    arguments: args,
                                });
                            }
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }

    None
}

/// Inject a detected tool call into an OpenAI-format response body.
fn inject_tool_call(body: &mut Value, tool_call: ToolCall) {
    if let Some(choices) = body
        .as_object_mut()
        .and_then(|m| m.get_mut("choices"))
        .and_then(|c| c.as_array_mut())
    {
        if let Some(first) = choices.first_mut() {
            if let Some(msg) = first.get_mut("message") {
                let tool_calls = json!([{
                    "id": format!("call_{}", uuid_v4_short()),
                    "type": "function",
                    "function": {
                        "name": tool_call.name,
                        "arguments": tool_call.arguments.to_string()
                    }
                }]);
                msg["tool_calls"] = tool_calls;
            }
        }
    }
}

fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", nanos)[..12].to_string()
}

#[derive(Debug)]
struct ToolCall {
    name: String,
    arguments: Value,
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Capability Matrix ───────────────────────────────────

    #[test]
    fn test_openai_native_except_prefix() {
        let caps = capabilities("openai");
        for (name, support) in &caps {
            if *name == "prefix" {
                assert_eq!(
                    *support,
                    Support::Fallback(FallbackStrategy::StripWarn),
                    "OpenAI doesn't support 'prefix' natively"
                );
            } else {
                assert_eq!(
                    *support,
                    Support::Native,
                    "OpenAI should support '{}' natively",
                    name
                );
            }
        }
    }

    #[test]
    fn test_anthropic_strips_frequency_penalty() {
        let caps = capabilities("anthropic");
        let fp = caps
            .iter()
            .find(|(n, _)| *n == "frequency_penalty")
            .unwrap();
        assert_eq!(fp.1, Support::Fallback(FallbackStrategy::StripWarn));
    }

    #[test]
    fn test_ollama_emulates_tool_choice() {
        let caps = capabilities("ollama");
        let tc = caps.iter().find(|(n, _)| *n == "tool_choice").unwrap();
        assert_eq!(tc.1, Support::Fallback(FallbackStrategy::ResponseTransform));
    }

    // ── Request Virtualization ──────────────────────────────

    #[test]
    fn test_strip_unsupported_field() {
        let mut body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}],
            "frequency_penalty": 0.5
        });
        let warns = virtualize_request("anthropic", &mut body);
        assert!(body.get("frequency_penalty").is_none());
        assert!(!warns.is_empty());
        assert!(warns[0].contains("frequency_penalty"));
    }

    #[test]
    fn test_leave_native_fields() {
        let mut body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hi"}],
            "frequency_penalty": 0.5,
            "seed": 42
        });
        let warns = virtualize_request("openai", &mut body);
        assert!(warns.is_empty());
        assert!(body.get("frequency_penalty").is_some());
        assert!(body.get("seed").is_some());
    }

    #[test]
    fn test_prompt_inject_for_response_format() {
        let mut body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hello"}],
            "response_format": {"type": "json_object"}
        });
        let warns = virtualize_request("anthropic", &mut body);
        assert!(body.get("response_format").is_none());
        // Should have injected into system prompt
        let msgs = body.get("messages").and_then(|m| m.as_array()).unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[0]["content"].as_str().unwrap().contains("JSON"));
        assert!(!warns.is_empty());
    }

    #[test]
    fn test_prompt_inject_appends_to_existing_system() {
        let mut body = json!({
            "model": "claude-3",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "hello"}
            ],
            "response_format": {"type": "json_object"}
        });
        virtualize_request("anthropic", &mut body);
        let msgs = body.get("messages").and_then(|m| m.as_array()).unwrap();
        assert_eq!(msgs.len(), 2); // still 2 messages
        assert!(
            msgs[0]["content"]
                .as_str()
                .unwrap()
                .contains("You are helpful")
        );
        assert!(msgs[0]["content"].as_str().unwrap().contains("JSON"));
    }

    #[test]
    fn test_deepseek_strips_seed_but_keeps_frequency() {
        let mut body = json!({
            "model": "deepseek-chat",
            "messages": [{"role": "user", "content": "hi"}],
            "seed": 42,
            "frequency_penalty": 0.5,
            "prefix": true
        });
        let warns = virtualize_request("deepseek", &mut body);
        assert!(body.get("seed").is_none()); // stripped
        assert_eq!(body.get("frequency_penalty"), Some(&json!(0.5))); // kept
        assert_eq!(body.get("prefix"), Some(&json!(true))); // kept (native)
        assert!(warns.iter().any(|w| w.contains("seed")));
    }

    // ── Response Virtualization ─────────────────────────────

    #[test]
    fn test_extract_tool_call_from_json_in_text() {
        let body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "I'll look that up for you.\n{\"function\": \"search_weather\", \"arguments\": {\"location\": \"Paris\"}}"
                }
            }]
        });
        let result = extract_tool_call_from_text(&body);
        assert!(result.is_some());
        let tc = result.unwrap();
        assert_eq!(tc.name, "search_weather");
        assert_eq!(tc.arguments["location"], "Paris");
    }

    #[test]
    fn test_inject_tool_call_into_response() {
        let mut body = json!({
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Let me check"
                },
                "finish_reason": "stop"
            }]
        });
        inject_tool_call(
            &mut body,
            ToolCall {
                name: "get_weather".into(),
                arguments: json!({"city": "London"}),
            },
        );
        let tc = &body["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], "get_weather");
        assert_eq!(tc["type"], "function");
        assert!(tc["id"].as_str().unwrap().starts_with("call_"));
    }

    #[test]
    fn test_response_virtualize_tool_call() {
        let mut body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "{\"function\": \"get_weather\", \"arguments\": {\"city\": \"Tokyo\"}}"
                }
            }]
        });
        let warns = virtualize_response("ollama", &mut body, &["tool_choice"]);
        assert!(!warns.is_empty());
        let tc = &body["choices"][0]["message"]["tool_calls"];
        assert!(tc.is_array());
        assert_eq!(tc[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_noop_for_unsupported_feature_when_not_requested() {
        let mut body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}]
            // No frequency_penalty field at all
        });
        let warns = virtualize_request("anthropic", &mut body);
        // Should be empty — no warnings needed for features not requested
        assert_eq!(
            warns.len(),
            0,
            "no warnings when unsupported features aren't used"
        );
    }

    #[test]
    fn test_capabilities_unknown_provider() {
        let caps = capabilities("some-new-provider");
        assert!(caps.is_empty());
    }
}
