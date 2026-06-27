//! Provider Schema Adapters — map between OpenAI-compatible format and provider-specific schemas.
//!
//! # Architecture
//!
//! Portail uses **OpenAI-compatible** as its canonical internal format for chat completions.
//! When a request arrives in OpenAI format and the upstream is Anthropic/Gemini/Ollama,
//! the adapter transforms the request body to match the provider's expected schema.
//! On the response path, the adapter transforms the provider-specific response back
//! to OpenAI format so the client sees a consistent API.
//!
//! # Adding a new provider
//!
//! 1. Implement `ProviderAdapter` for your provider
//! 2. Register it in `registry()`
//! 3. Add a `provider_path()` entry in `target_router.rs` if path differs

use serde_json::{Value, json};

/// Provider adapter trait — transforms requests/responses between OpenAI format
/// and provider-specific formats.
pub trait ProviderAdapter: Send + Sync {
    /// Provider name, e.g. "openai", "deepseek", "anthropic", "google", "ollama"
    fn name(&self) -> &'static str;

    /// Transform an OpenAI-format request body to this provider's format.
    /// Called before forwarding to upstream.
    fn adapt_request(&self, body: &mut Value) -> Result<(), String>;

    /// Transform this provider's response body back to OpenAI-compatible format.
    /// Called after receiving the upstream response.
    fn adapt_response(&self, body: &mut Value) -> Result<(), String>;

    /// Provider-specific headers to inject on every request (e.g. `anthropic-version`).
    fn extra_headers(&self) -> Vec<(&'static str, String)> {
        vec![]
    }

    /// Default parameters to merge into every request body.
    fn default_params(&self) -> Value {
        json!({})
    }
}

// ── Registry ──────────────────────────────────────────────────────────

static REGISTRY: std::sync::OnceLock<Vec<Box<dyn ProviderAdapter>>> = std::sync::OnceLock::new();

/// Get the provider adapter registry, initializing on first access.
pub fn registry() -> &'static [Box<dyn ProviderAdapter>] {
    REGISTRY.get_or_init(|| {
        vec![
            Box::new(OpenAIAdapter),
            Box::new(DeepSeekAdapter),
            Box::new(AnthropicAdapter),
            Box::new(GoogleAdapter),
            Box::new(OllamaAdapter),
        ]
    })
}

/// Resolve a provider adapter by name. Falls back to OpenAI if unknown.
pub fn by_name(name: &str) -> &'static dyn ProviderAdapter {
    registry()
        .iter()
        .find(|a| a.name() == name)
        .map(|a| a.as_ref())
        .unwrap_or(&OpenAIAdapter)
}

// ── OpenAI (canonical baseline) ──────────────────────────────────────

struct OpenAIAdapter;

impl ProviderAdapter for OpenAIAdapter {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn adapt_request(&self, _body: &mut Value) -> Result<(), String> {
        Ok(()) // OpenAI is the canonical format — no transformation needed
    }

    fn adapt_response(&self, _body: &mut Value) -> Result<(), String> {
        Ok(()) // Already in OpenAI format
    }
}

// ── DeepSeek (OpenAI-compatible + extras) ──────────────────────────

struct DeepSeekAdapter;

impl ProviderAdapter for DeepSeekAdapter {
    fn name(&self) -> &'static str {
        "deepseek"
    }

    fn adapt_request(&self, _body: &mut Value) -> Result<(), String> {
        // DeepSeek is almost identical to OpenAI. The key difference:
        // - `prefix` parameter enables prefix completion (alternative to system prompt)
        // - `frequence_penalty` is supported
        // We just pass through, same as OpenAI. DeepSeek handles OpenAI-format natively.
        Ok(())
    }

    fn adapt_response(&self, body: &mut Value) -> Result<(), String> {
        // DeepSeek returns OpenAI-compatible responses, but with some extras:
        // - `usage.completion_tokens_details.reasoning_tokens` for reasoning models
        // We keep the response mostly as-is, just ensuring the OpenAI-compatible fields are present.
        if let Some(usage) = body.get_mut("usage") {
            // Map any reasoning_tokens field for visibility
            if usage.get("completion_tokens_details").is_none() {
                if let Some(details) = usage.as_object_mut() {
                    details.insert(
                        "completion_tokens_details".into(),
                        json!({"reasoning_tokens": 0}),
                    );
                }
            }
        }
        Ok(())
    }
}

// ── Anthropic (major schema differences) ─────────────────────────────

struct AnthropicAdapter;

impl ProviderAdapter for AnthropicAdapter {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn adapt_request(&self, body: &mut Value) -> Result<(), String> {
        // OpenAI messages → Anthropic messages + system
        // Key differences:
        //   - system message → top-level "system" field
        //   - No assistant prefix support (merge into content)
        //   - max_tokens is REQUIRED for Anthropic

        let messages = body
            .get("messages")
            .and_then(|m| m.as_array())
            .ok_or_else(|| "missing messages array".to_string())?;

        let mut anthro_messages = Vec::new();
        let mut system_text = String::new();

        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

            if role == "system" {
                // Anthropic uses top-level "system" param instead of system messages
                if !system_text.is_empty() {
                    system_text.push('\n');
                }
                system_text.push_str(content);
            } else {
                // Map OpenAI roles to Anthropic: "assistant" stays, "user" stays
                let anthro_role = if role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                anthro_messages.push(json!({
                    "role": anthro_role,
                    "content": content
                }));
            }
        }

        // Rebuild body in Anthropic format
        if let Some(map) = body.as_object_mut() {
            map.insert("messages".into(), json!(anthro_messages));
            if !system_text.is_empty() {
                map.insert("system".into(), json!(system_text));
            }
            // Remove fields Anthropic doesn't understand
            map.remove("frequency_penalty");
            map.remove("presence_penalty");
            map.remove("logit_bias");
            map.remove("seed");
            map.remove("response_format");
            map.remove("tools");
            map.remove("tool_choice");

            // Anthropic requires max_tokens
            if map.get("max_tokens").is_none() {
                map.insert("max_tokens".into(), json!(4096));
            }
        }

        Ok(())
    }

    fn adapt_response(&self, body: &mut Value) -> Result<(), String> {
        // Anthropic response → OpenAI format
        // Anthropic: { id, type: "message", role: "assistant", content: [{type:"text", text:"..."}], ... }
        // OpenAI:    { id, object: "chat.completion", choices: [{index, message: {role, content}, finish_reason}], usage }

        let content_text = body
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let finish_reason = match body.get("stop_reason").and_then(|s| s.as_str()) {
            Some("end_turn") | Some("stop_sequence") => "stop".to_string(),
            Some("max_tokens") => "length".to_string(),
            Some("tool_use") => "tool_calls".to_string(),
            _ => "stop".to_string(),
        };

        let input_tokens = body
            .get("usage")
            .and_then(|u| u.get("input_tokens"))
            .cloned();
        let output_tokens = body
            .get("usage")
            .and_then(|u| u.get("output_tokens"))
            .cloned();

        if let Some(map) = body.as_object_mut() {
            map.insert("object".into(), json!("chat.completion"));
            map.insert(
                "choices".into(),
                json!([{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": content_text
                    },
                    "finish_reason": finish_reason
                }]),
            );
            // Clean up Anthropic-specific fields
            map.remove("content");
            map.remove("stop_reason");
            map.remove("type");
            map.remove("role");

            // Map usage
            if let Some(ref mut usage) = map.get_mut("usage") {
                if let Some(u) = usage.as_object_mut() {
                    if input_tokens.is_some() {
                        u.insert(
                            "prompt_tokens".into(),
                            input_tokens.clone().unwrap_or(json!(0)),
                        );
                    }
                    if output_tokens.is_some() {
                        u.insert(
                            "completion_tokens".into(),
                            output_tokens.clone().unwrap_or(json!(0)),
                        );
                    }
                    u.remove("input_tokens");
                    u.remove("output_tokens");
                }
            } else {
                map.insert(
                    "usage".into(),
                    json!({
                        "prompt_tokens": input_tokens.unwrap_or(json!(0)),
                        "completion_tokens": output_tokens.unwrap_or(json!(0)),
                        "total_tokens": json!(0)
                    }),
                );
            }
        }

        Ok(())
    }

    fn extra_headers(&self) -> Vec<(&'static str, String)> {
        vec![
            ("anthropic-version", "2023-06-01".to_string()),
            (
                "x-api-key",
                std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            ),
        ]
    }

    fn default_params(&self) -> Value {
        json!({
            "max_tokens": 4096
        })
    }
}

// ── Google Gemini (different nested structure) ──────────────────────

struct GoogleAdapter;

impl ProviderAdapter for GoogleAdapter {
    fn name(&self) -> &'static str {
        "google"
    }

    fn adapt_request(&self, body: &mut Value) -> Result<(), String> {
        // OpenAI format → Gemini format
        // OpenAI: { model, messages: [{role, content}], max_tokens, temperature, stream }
        // Gemini: { contents: [{role, parts: [{text}]}], generationConfig: {maxOutputTokens, temperature} }

        let messages = body
            .get("messages")
            .and_then(|m| m.as_array())
            .ok_or_else(|| "missing messages array".to_string())?;

        let mut contents = Vec::new();
        let mut system_text = String::new();

        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

            if role == "system" {
                if !system_text.is_empty() {
                    system_text.push('\n');
                }
                system_text.push_str(content);
                continue;
            }

            // Gemini maps: "assistant" → "model", "user" → "user"
            let gemini_role = if role == "assistant" { "model" } else { "user" };
            contents.push(json!({
                "role": gemini_role,
                "parts": [{"text": content}]
            }));
        }

        let mut gen_config = json!({});
        if let Some(v) = body.get("max_tokens").and_then(|v| v.as_u64()) {
            gen_config["maxOutputTokens"] = json!(v);
        }
        if let Some(v) = body.get("temperature").and_then(|v| v.as_f64()) {
            gen_config["temperature"] = json!(v);
        }
        if let Some(v) = body.get("top_p").and_then(|v| v.as_f64()) {
            gen_config["topP"] = json!(v);
        }
        if let Some(v) = body.get("top_k").and_then(|v| v.as_u64()) {
            gen_config["topK"] = json!(v);
        }

        if let Some(map) = body.as_object_mut() {
            map.insert("contents".into(), json!(contents));
            map.remove("messages");
            map.remove("max_tokens");
            map.remove("frequency_penalty");
            map.remove("presence_penalty");

            if !system_text.is_empty() {
                map.insert(
                    "system_instruction".into(),
                    json!({
                        "parts": [{"text": system_text}]
                    }),
                );
            }

            if gen_config.as_object().is_some_and(|o| !o.is_empty()) {
                map.insert("generationConfig".into(), gen_config);
            }
        }

        Ok(())
    }

    fn adapt_response(&self, body: &mut Value) -> Result<(), String> {
        // Gemini response → OpenAI format
        // Gemini: { candidates: [{ content: { parts: [{text:...}], role: "model" }, finishReason }] }
        // OpenAI: { choices: [{ message: { role, content }, finish_reason }] }

        let candidates = body.get("candidates").and_then(|c| c.as_array());
        let choices: Vec<Value> = match candidates {
            Some(cands) => cands
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let text = c
                        .get("content")
                        .and_then(|ct| ct.get("parts"))
                        .and_then(|p| p.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|first| first.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    let finish = c
                        .get("finishReason")
                        .and_then(|f| f.as_str())
                        .map(|f| match f {
                            "STOP" => "stop",
                            "MAX_TOKENS" => "length",
                            "SAFETY" => "content_filter",
                            "RECITATION" => "content_filter",
                            _ => "stop",
                        })
                        .unwrap_or("stop");
                    json!({
                        "index": i,
                        "message": {
                            "role": "assistant",
                            "content": text
                        },
                        "finish_reason": finish
                    })
                })
                .collect(),
            None => vec![],
        };

        // Extract usage metadata before mutable borrow
        let usage_prompt = body
            .get("usageMetadata")
            .and_then(|m| m.get("promptTokenCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let usage_output = body
            .get("usageMetadata")
            .and_then(|m| m.get("candidatesTokenCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if let Some(map) = body.as_object_mut() {
            map.insert("object".into(), json!("chat.completion"));
            map.insert("choices".into(), json!(choices));
            map.remove("candidates");

            if usage_prompt > 0 || usage_output > 0 {
                map.insert(
                    "usage".into(),
                    json!({
                        "prompt_tokens": usage_prompt,
                        "completion_tokens": usage_output,
                        "total_tokens": usage_prompt + usage_output
                    }),
                );
            }
            map.remove("usageMetadata");
        }

        Ok(())
    }
}

// ── Ollama (simpler schema) ──────────────────────────────────────────

struct OllamaAdapter;

impl ProviderAdapter for OllamaAdapter {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn adapt_request(&self, body: &mut Value) -> Result<(), String> {
        // OpenAI format → Ollama format
        // Key difference: Ollama messages use role "user"/"assistant"/"system" same as OpenAI,
        // but wraps parameters differently.
        //
        // OpenAI:  { model, messages, max_tokens, temperature, stream }
        // Ollama:  { model, messages, stream, options: { num_predict, temperature } }

        if let Some(map) = body.as_object_mut() {
            let mut options = json!({});

            if let Some(v) = map.remove("max_tokens") {
                options["num_predict"] = v;
            }
            if let Some(v) = map.remove("temperature") {
                options["temperature"] = v;
            }
            if let Some(v) = map.remove("top_p") {
                options["top_p"] = v;
            }
            if let Some(v) = map.remove("frequency_penalty") {
                options["frequency_penalty"] = v;
            }
            if let Some(v) = map.remove("presence_penalty") {
                options["presence_penalty"] = v;
            }
            if let Some(v) = map.remove("seed") {
                options["seed"] = v;
            }
            // Ollama doesn't support these
            map.remove("logit_bias");
            map.remove("response_format");
            map.remove("tools");
            map.remove("tool_choice");

            if options.as_object().is_some_and(|o| !o.is_empty()) {
                map.insert("options".into(), options);
            }
        }

        Ok(())
    }

    fn adapt_response(&self, body: &mut Value) -> Result<(), String> {
        // Ollama response → OpenAI format
        // Ollama: { model, created_at, message: { role, content }, done, total_duration, ... }
        // Some models (qwen3) also emit: message: { role, content: "", thinking: "...chain..." }
        // OpenAI: { choices: [{ message: { role, content }, finish_reason }], usage }

        // Extract thinking field before it gets lost
        let thinking_text = body
            .get("message")
            .and_then(|m| m.get("thinking"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        let mut msg = body.get("message").cloned().unwrap_or(json!({
            "role": "assistant",
            "content": ""
        }));

        // If thinking exists but content is empty, promote thinking → content
        if let Some(ref thinking) = thinking_text {
            let existing = msg["content"].as_str().unwrap_or("").to_string();
            if existing.is_empty() {
                msg["content"] = json!(thinking);
            } else {
                msg["content"] = json!(format!(
                    "{}\n\n[thinking]\n{}[/thinking]",
                    existing, thinking
                ));
            }
            // Also add thinking as a separate field for clients that support it
            msg["thinking"] = json!(thinking);
        }

        let finish = body
            .get("done_reason")
            .and_then(|r| r.as_str())
            .unwrap_or("stop")
            .to_string();

        let eval_count = body.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let prompt_eval_count = body
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if let Some(map) = body.as_object_mut() {
            map.insert("object".into(), json!("chat.completion"));
            map.insert(
                "choices".into(),
                json!([{
                    "index": 0,
                    "message": msg,
                    "finish_reason": finish
                }]),
            );
            map.insert(
                "usage".into(),
                json!({
                    "prompt_tokens": prompt_eval_count,
                    "completion_tokens": eval_count,
                    "total_tokens": prompt_eval_count + eval_count
                }),
            );
            // Clean up Ollama-specific fields
            map.remove("message");
            map.remove("done");
            map.remove("done_reason");
            map.remove("eval_count");
            map.remove("eval_duration");
            map.remove("prompt_eval_count");
            map.remove("prompt_eval_duration");
            map.remove("total_duration");
            map.remove("load_duration");
            map.remove("created_at");
        }

        Ok(())
    }

    fn default_params(&self) -> Value {
        json!({
            "options": {
                "num_predict": 2048,
                "temperature": 0.7
            }
        })
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_contains_all() {
        let names: Vec<&str> = registry().iter().map(|a| a.name()).collect();
        assert!(names.contains(&"openai"));
        assert!(names.contains(&"deepseek"));
        assert!(names.contains(&"anthropic"));
        assert!(names.contains(&"google"));
        assert!(names.contains(&"ollama"));
    }

    #[test]
    fn test_by_name_fallback() {
        assert_eq!(by_name("openai").name(), "openai");
        assert_eq!(by_name("anthropic").name(), "anthropic");
        assert_eq!(by_name("unknown_provider").name(), "openai"); // fallback
    }

    #[test]
    fn test_openai_passthrough() {
        let adapter = OpenAIAdapter;
        let mut body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 100
        });
        let original = body.clone();
        adapter.adapt_request(&mut body).unwrap();
        assert_eq!(body, original); // no transformation
        adapter.adapt_response(&mut body).unwrap();
        assert_eq!(body, original); // no transformation
    }

    #[test]
    fn test_anthropic_request_maps_system_message() {
        let adapter = AnthropicAdapter;
        let mut body = json!({
            "model": "claude-sonnet-4",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant"},
                {"role": "user", "content": "Hello"}
            ],
            "max_tokens": 100,
            "temperature": 0.7
        });
        adapter.adapt_request(&mut body).unwrap();
        assert_eq!(body["system"], "You are a helpful assistant");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hello");
        assert_eq!(body["max_tokens"], 100);
    }

    #[test]
    fn test_anthropic_request_strips_openai_only_fields() {
        let adapter = AnthropicAdapter;
        let mut body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}],
            "frequency_penalty": 0.5,
            "seed": 42,
            "response_format": {"type": "json_object"}
        });
        adapter.adapt_request(&mut body).unwrap();
        assert!(body.get("frequency_penalty").is_none());
        assert!(body.get("seed").is_none());
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn test_anthropic_request_adds_max_tokens() {
        let adapter = AnthropicAdapter;
        let mut body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}]
        });
        adapter.adapt_request(&mut body).unwrap();
        assert_eq!(body["max_tokens"], 4096);
    }

    #[test]
    fn test_anthropic_response_transforms() {
        let adapter = AnthropicAdapter;
        let mut body = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello world"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 20}
        });
        adapter.adapt_response(&mut body).unwrap();
        assert_eq!(body["choices"][0]["message"]["content"], "Hello world");
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
        assert_eq!(body["usage"]["prompt_tokens"], 10);
        assert_eq!(body["usage"]["completion_tokens"], 20);
        assert!(body.get("stop_reason").is_none());
        assert!(body.get("type").is_none());
    }

    #[test]
    fn test_google_request_transforms() {
        let adapter = GoogleAdapter;
        let mut body = json!({
            "model": "gemini-2.5-flash",
            "messages": [
                {"role": "system", "content": "Be concise"},
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi"}
            ],
            "max_tokens": 200,
            "temperature": 0.5
        });
        adapter.adapt_request(&mut body).unwrap();
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][1]["role"], "model");
        assert_eq!(body["system_instruction"]["parts"][0]["text"], "Be concise");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 200);
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn test_google_response_transforms() {
        let adapter = GoogleAdapter;
        let mut body = json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Gemini"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 15,
                "candidatesTokenCount": 25
            }
        });
        adapter.adapt_response(&mut body).unwrap();
        assert_eq!(
            body["choices"][0]["message"]["content"],
            "Hello from Gemini"
        );
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
        assert_eq!(body["usage"]["prompt_tokens"], 15);
        assert_eq!(body["usage"]["completion_tokens"], 25);
        assert!(body.get("candidates").is_none());
    }

    #[test]
    fn test_ollama_request_wraps_options() {
        let adapter = OllamaAdapter;
        let mut body = json!({
            "model": "llama3",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 512,
            "temperature": 0.8,
            "seed": 123
        });
        adapter.adapt_request(&mut body).unwrap();
        assert_eq!(body["options"]["num_predict"], 512);
        assert_eq!(body["options"]["temperature"], 0.8);
        assert_eq!(body["options"]["seed"], 123);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn test_ollama_response_transforms() {
        let adapter = OllamaAdapter;
        let mut body = json!({
            "model": "llama3",
            "message": {"role": "assistant", "content": "Hello from Ollama"},
            "done_reason": "stop",
            "eval_count": 50,
            "prompt_eval_count": 10,
            "total_duration": 1234567890
        });
        adapter.adapt_response(&mut body).unwrap();
        assert_eq!(
            body["choices"][0]["message"]["content"],
            "Hello from Ollama"
        );
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
        assert_eq!(body["usage"]["prompt_tokens"], 10);
        assert_eq!(body["usage"]["completion_tokens"], 50);
        assert!(body.get("message").is_none());
        assert!(body.get("done_reason").is_none());
    }

    #[test]
    fn test_ollama_response_with_thinking_promoted_to_content() {
        // qwen3 models emit content="" + thinking field
        let adapter = OllamaAdapter;
        let mut body = json!({
            "model": "qwen3:8b",
            "message": {
                "role": "assistant",
                "content": "",
                "thinking": "Okay, the user wants hello in one word. Hello is one word."
            },
            "done_reason": "stop",
            "eval_count": 50,
            "prompt_eval_count": 10
        });
        adapter.adapt_response(&mut body).unwrap();
        // Thinking should be promoted to content since content was empty
        assert!(
            body["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
                .contains("Hello is one word")
        );
        // Thinking should also be available as a separate field
        assert!(
            body["choices"][0]["message"]["thinking"]
                .as_str()
                .unwrap()
                .contains("Okay")
        );
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_ollama_response_with_both_content_and_thinking() {
        // Some models return both content AND thinking
        let adapter = OllamaAdapter;
        let mut body = json!({
            "model": "qwen3:8b",
            "message": {
                "role": "assistant",
                "content": "Some visible output",
                "thinking": "Internal reasoning chain here"
            },
            "done_reason": "stop",
            "eval_count": 100,
            "prompt_eval_count": 20
        });
        adapter.adapt_response(&mut body).unwrap();
        // Content should be preserved with thinking appended
        let content = body["choices"][0]["message"]["content"].as_str().unwrap();
        assert!(content.contains("Some visible output"));
        assert!(content.contains("[thinking]"));
        assert!(content.contains("Internal reasoning chain"));
        // Thinking field should be present
        assert!(
            body["choices"][0]["message"]["thinking"]
                .as_str()
                .unwrap()
                .contains("Internal reasoning")
        );
    }

    #[test]
    fn test_anthropic_streaming_response() {
        // SSE streaming responses are handled at a higher level (gateway.rs)
        // This tests the non-streaming response format
        let adapter = AnthropicAdapter;
        let mut body = json!({
            "content": [{"type": "text", "text": "Stream response"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 5, "output_tokens": 100}
        });
        adapter.adapt_response(&mut body).unwrap();
        assert_eq!(body["choices"][0]["finish_reason"], "length");
        assert_eq!(body["choices"][0]["message"]["content"], "Stream response");
    }

    #[test]
    fn test_anthropic_only_user_messages() {
        let adapter = AnthropicAdapter;
        let mut body = json!({
            "messages": [
                {"role": "user", "content": "First"},
                {"role": "assistant", "content": "Middle"},
                {"role": "user", "content": "Last"}
            ]
        });
        adapter.adapt_request(&mut body).unwrap();
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][1]["role"], "assistant");
    }
}
