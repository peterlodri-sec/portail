//! E2E tests - v3.0 AI-NATIVE features.
//!
//! Covers: function calling, tool virtualization, cost attribution,
//! session tracking, prompt hooks, gateway schema adapters.

#[cfg(test)]
mod e2e_v3 {
    use std::sync::Arc;
    use std::sync::RwLock;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;

    use portail::*;

    fn base_state() -> AppState {
        AppState {
            config: RwLock::new(config::Config::default()),
            event_log: Arc::new(events::EventLog::new(100)),
            cdn_cache: None,
            hooks: Arc::new(hooks::HookStore::new()),
            base_hooks: Arc::new(portail::base_hooks::default_registry()),
            a2a_tasks: Arc::new(a2a::TaskStore::new()),
            a2a_registry: Arc::new(a2a::registry::AgentRegistry::new()),
            dns_store: Arc::new(dns::DnsStore::new()),
            doh_client: None,
            network_isolation: Arc::new(dns::NetworkIsolation::default()),
            tinyurl: Arc::new(plugins::TinyUrlStore::new(plugins::TinyUrlConfig::default())),
            trace_store: Arc::new(plugins::TraceStore::new(100)),
            redis_cache: Arc::new(plugins::RedisCache::new(
                plugins::RedisCacheConfig::default(),
            )),
            discovery: Arc::new(discovery::DiscoveryStore::new(
                discovery::DiscoveryConfig::default(),
            )),
            ci_status: Arc::new(ci::CiStatusStore::new(100, None)),
            metrics_handle: test_utils::global_metrics().clone(),
            rate_limiter: None,
            auth_state: None,
            event_store: None,
            session_store: sessions::SessionStore::new(20),
            file_cache: file_cache::FileCache::new(&file_cache::FileCacheConfig {
                path: "/tmp/portail-e2e-v3-cache".into(),
                ..Default::default()
            }),
            config_watcher: config_watcher::ConfigWatcher::new(std::path::PathBuf::from(
                "portail.toml",
            )),
            supervisor: Arc::new(supervisor::Supervisor::new(Arc::new(
                events::EventLog::new(100),
            ))),
            plugin_registry: plugin_hooks::init_plugin_registry(std::path::Path::new("vaked")),
            loop_manager: Arc::new(loop_state_manager::LoopStateManager::new("3.0.0")),
            loop_runner: loopeng::SharedLoopEngine::new(loopeng::LoopEngineConfig::default()),
            inference_engine: None,
            pkg_ctx_memory: tokio::sync::Mutex::new(pkg_ctx::memory::PkgCtxMemory::new().unwrap()),
            tool_registry: Arc::new(RwLock::new(
                portail_claude_plugins::bridge::ToolRegistry::new(),
            )),
        }
    }

    // -- 1. Function calling - tool_choice virtualization --

    #[tokio::test]
    async fn tool_choice_request_virtualized_for_ollama() {
        use portail::gateway::features::{FallbackStrategy, Support, virtualize_request};

        let caps = portail::gateway::features::capabilities("ollama");
        let tc = caps.iter().find(|(n, _)| *n == "tool_choice").unwrap();
        assert!(
            matches!(tc.1, Support::Fallback(FallbackStrategy::ResponseTransform)),
            "ollama tool_choice must use ResponseTransform fallback"
        );

        let mut body = json!({
            "model": "llama3",
            "messages": [{"role": "user", "content": "what is the weather?"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
                }
            }],
            "tool_choice": "auto"
        });

        let warnings = virtualize_request("ollama", &mut body);
        assert!(body.get("tool_choice").is_none());
        assert!(warnings.iter().any(|w| w.contains("tool_choice")));
    }

    #[tokio::test]
    async fn tool_call_extracted_from_text_response() {
        use portail::gateway::features::virtualize_response;

        let mut body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "{\"function\": \"get_weather\", \"arguments\": {\"location\": \"Paris\"}}"
                }
            }]
        });

        let warns = virtualize_response("ollama", &mut body, &["tool_choice"]);
        assert!(!warns.is_empty());
        let tool_calls = &body["choices"][0]["message"]["tool_calls"];
        assert!(tool_calls.is_array());
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        assert_eq!(tool_calls[0]["type"], "function");
    }

    #[tokio::test]
    async fn tool_choice_native_for_openai() {
        use portail::gateway::features::Support;

        let caps = portail::gateway::features::capabilities("openai");
        let tc = caps.iter().find(|(n, _)| *n == "tool_choice").unwrap();
        assert!(matches!(tc.1, Support::Native));

        let mut body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hello"}],
            "tool_choice": "auto"
        });
        let warns = portail::gateway::features::virtualize_request("openai", &mut body);
        assert!(body.get("tool_choice").is_some());
        assert!(warns.is_empty());
    }

    // -- 2. Provider schema adapters - roundtrip --

    #[tokio::test]
    async fn anthropic_adapter_system_message_roundtrip() {
        use portail::gateway::schema::by_name;

        let adapter = by_name("anthropic");
        let mut req = json!({
            "model": "claude-3",
            "messages": [
                {"role": "system", "content": "be helpful"},
                {"role": "user", "content": "hi"}
            ]
        });
        adapter.adapt_request(&mut req).unwrap();
        assert_eq!(req["system"], "be helpful");
        assert!(req.get("tools").is_none());
        assert_eq!(req["messages"][0]["role"], "user");

        let mut resp = json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "hello back"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 5, "output_tokens": 3}
        });
        adapter.adapt_response(&mut resp).unwrap();
        assert_eq!(resp["choices"][0]["message"]["content"], "hello back");
        assert_eq!(resp["choices"][0]["finish_reason"], "stop");
        assert_eq!(resp["usage"]["prompt_tokens"], 5);
        assert_eq!(resp["usage"]["completion_tokens"], 3);
    }

    #[tokio::test]
    async fn ollama_adapter_thinking_field_preserved() {
        use portail::gateway::schema::by_name;

        let adapter = by_name("ollama");
        let mut req = json!({
            "model": "qwen3",
            "messages": [{"role": "user", "content": "think"}],
            "max_tokens": 100
        });
        adapter.adapt_request(&mut req).unwrap();
        assert!(req.get("options").is_some());
        assert_eq!(req["options"]["num_predict"], 100);

        let mut resp = json!({
            "model": "qwen3",
            "message": {
                "role": "assistant",
                "content": "",
                "thinking": "let me analyze this..."
            },
            "done": true,
            "eval_count": 50,
            "prompt_eval_count": 10
        });
        adapter.adapt_response(&mut resp).unwrap();
        let content = resp["choices"][0]["message"]["content"].as_str().unwrap();
        assert!(content.contains("let me analyze"));
        assert!(resp["choices"][0]["message"].get("thinking").is_some());
        assert_eq!(resp["usage"]["completion_tokens"], 50);
    }

    #[tokio::test]
    async fn google_adapter_contents_format() {
        use portail::gateway::schema::by_name;

        let adapter = by_name("google");
        let mut req = json!({
            "model": "gemini-2.5-flash",
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hello"}
            ],
            "max_tokens": 50,
            "temperature": 0.5
        });
        adapter.adapt_request(&mut req).unwrap();
        assert!(req.get("contents").is_some());
        assert_eq!(req["contents"][0]["role"], "user");
        assert_eq!(req["contents"][0]["parts"][0]["text"], "hello");
        assert!(req.get("system_instruction").is_some());
        assert!(req.get("generationConfig").is_some());
        assert_eq!(req["generationConfig"]["maxOutputTokens"], 50);
    }

    // -- 3. Cost attribution - session tracking --

    #[tokio::test]
    async fn session_records_token_counts() {
        let state = base_state();
        let session_id = "cost-session-1";

        state.session_store.record_request(
            session_id,
            "POST",
            "/v1/chat/completions",
            200,
            std::time::Duration::from_millis(150),
            std::time::Duration::from_micros(500),
            1000,
            500,
            false,
            0,
        );

        let summary = state.session_store.list_sessions();
        let found = summary.iter().find(|s| s.session_id == session_id);
        assert!(found.is_some(), "session must exist after recording");

        let stats = state.session_store.get_session(session_id);
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.request_count, 1);
        assert_eq!(stats.total_input_tokens, 1000);
        assert_eq!(stats.total_output_tokens, 500);
    }

    #[tokio::test]
    async fn session_accumulates_multiple_requests() {
        let state = base_state();
        let sid = "cost-session-2";

        for i in 0..5 {
            state.session_store.record_request(
                sid,
                "POST",
                "/v1/chat/completions",
                200,
                std::time::Duration::from_millis(100 + i * 10),
                std::time::Duration::from_micros(500),
                200,
                100,
                i % 2 == 0,
                0,
            );
        }

        let stats = state.session_store.get_session(sid).unwrap();
        assert_eq!(stats.request_count, 5);
        assert_eq!(stats.total_input_tokens, 1000);
        assert_eq!(stats.total_output_tokens, 500);
    }

    // -- 4. Prompt versioning - hooks as versioned injects --

    #[tokio::test]
    async fn hook_store_crud_lifecycle() {
        let store = hooks::HookStore::new();

        store.add(hooks::Hook {
            id: "v1-prompt".into(),
            match_agent: None,
            match_path: Some("/v1/chat/completions".into()),
            match_event_type: None,
            inject: hooks::InjectMode::Prepend,
            content: "You are a v1 assistant.".into(),
            enabled: true,
            priority: 0,
        });
        assert_eq!(store.list().len(), 1);

        let matched = store.match_message("/v1/chat/completions");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].content, "You are a v1 assistant.");

        store.remove("v1-prompt");
        assert_eq!(store.list().len(), 0);
        assert!(store.match_message("/v1/chat/completions").is_empty());
    }

    #[tokio::test]
    async fn multiple_hooks_ordered_by_priority() {
        let store = hooks::HookStore::new();

        store.add(hooks::Hook {
            id: "low-priority".into(),
            match_agent: None,
            match_path: Some("/test".into()),
            match_event_type: None,
            inject: hooks::InjectMode::Prepend,
            content: "low".into(),
            enabled: true,
            priority: 10,
        });
        store.add(hooks::Hook {
            id: "high-priority".into(),
            match_agent: None,
            match_path: Some("/test".into()),
            match_event_type: None,
            inject: hooks::InjectMode::Prepend,
            content: "high".into(),
            enabled: true,
            priority: 1,
        });

        let matched = store.match_message("/test");
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].content, "high");
        assert_eq!(matched[1].content, "low");
    }

    #[tokio::test]
    async fn apply_message_hooks_prepends_in_order() {
        let body = json!({
            "messages": [{"role": "user", "content": "original"}]
        });

        let hooks = vec![
            hooks::Hook {
                id: "h1".into(),
                match_agent: None,
                match_path: None,
                match_event_type: None,
                inject: hooks::InjectMode::Prepend,
                content: "first".into(),
                enabled: true,
                priority: 1,
            },
            hooks::Hook {
                id: "h2".into(),
                match_agent: None,
                match_path: None,
                match_event_type: None,
                inject: hooks::InjectMode::Prepend,
                content: "second".into(),
                enabled: true,
                priority: 2,
            },
        ];

        let result = hooks::apply_message_hooks(&body, &hooks).unwrap();
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["content"], "second");
        assert_eq!(msgs[1]["content"], "first");
        assert_eq!(msgs[2]["content"], "original");
    }

    // -- 5. Feature virtualization matrix --

    #[tokio::test]
    async fn all_providers_have_capabilities_defined() {
        let providers = ["openai", "deepseek", "anthropic", "google", "ollama"];
        for p in providers {
            let caps = portail::gateway::features::capabilities(p);
            assert!(!caps.is_empty(), "{} must have capabilities", p);
            let has_tool_choice = caps.iter().any(|(n, _)| *n == "tool_choice");
            assert!(has_tool_choice, "{} must declare tool_choice capability", p);
        }
    }

    #[tokio::test]
    async fn response_format_fallback_varies_by_provider() {
        use portail::gateway::features::{FallbackStrategy, Support};

        let openai_caps = portail::gateway::features::capabilities("openai");
        let rf = openai_caps
            .iter()
            .find(|(n, _)| *n == "response_format")
            .unwrap();
        assert!(matches!(rf.1, Support::Native));

        let anthropic_caps = portail::gateway::features::capabilities("anthropic");
        let rf = anthropic_caps
            .iter()
            .find(|(n, _)| *n == "response_format")
            .unwrap();
        assert!(matches!(
            rf.1,
            Support::Fallback(FallbackStrategy::PromptInject)
        ));

        let ollama_caps = portail::gateway::features::capabilities("ollama");
        let rf = ollama_caps
            .iter()
            .find(|(n, _)| *n == "response_format")
            .unwrap();
        assert!(matches!(
            rf.1,
            Support::Fallback(FallbackStrategy::PromptInject)
        ));
    }

    // -- 6. Gateway route handles tool-aware requests --

    #[tokio::test]
    async fn chat_completions_accepts_tool_definition() {
        let state = Arc::new(base_state());
        let app = proxy::build_router(state);

        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hello"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "test_tool",
                    "parameters": {}
                }
            }]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn embeddings_route_exists() {
        let state = Arc::new(base_state());
        let app = proxy::build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"model": "text-embedding-3-small", "input": "hello"}).to_string(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    // -- 7. Event log captures hook injection events --

    #[tokio::test]
    async fn event_log_records_hook_injection() {
        let state = base_state();

        state.event_log.publish(events::AgentEvent {
            agent_id: "hooks".into(),
            event_type: "injected".into(),
            severity: "info".into(),
            timestamp: 0,
            metadata: types::BoundedMeta::from_iter([
                ("path".into(), "/v1/chat/completions".into()),
                ("count".into(), "2".into()),
                ("hook_ids".into(), "v1-prompt,v2-system".into()),
            ]),
        });

        let recent = state.event_log.recent(10);
        let injected: Vec<_> = recent
            .iter()
            .filter(|e| e.event_type == "injected")
            .collect();
        assert!(!injected.is_empty());
        assert_eq!(injected[0].agent_id, "hooks");
    }

    // -- 8. Trace store - request tracing for cost debug --

    #[tokio::test]
    async fn trace_store_records_and_retrieves() {
        use portail::plugins::{Trace, TraceStore};

        let store = TraceStore::new(100);

        store.record(Trace {
            trace_id: "trace-1".into(),
            request_id: "req-1".into(),
            method: "POST".into(),
            path: "/v1/chat/completions".into(),
            status: 200,
            total_duration_us: 150_000,
            spans: vec![],
            metadata: types::BoundedMeta::default(),
            started_at: 0,
        });

        let traces = store.recent(10);
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_id, "trace-1");
        assert_eq!(traces[0].method, "POST");
        assert_eq!(traces[0].status, 200);
    }

    #[tokio::test]
    async fn trace_store_respects_capacity_limit() {
        use portail::plugins::{Trace, TraceStore};

        let store = TraceStore::new(3);

        for i in 0..5 {
            store.record(Trace {
                trace_id: format!("trace-{}", i),
                request_id: format!("req-{}", i),
                method: "POST".into(),
                path: "/v1/chat/completions".into(),
                status: 200,
                total_duration_us: 100_000,
                spans: vec![],
                metadata: types::BoundedMeta::default(),
                started_at: i as u64,
            });
        }

        let traces = store.recent(10);
        assert!(traces.len() <= 3, "trace store must cap at capacity");
    }

    // -- 9. Router - all v3.0 routes registered --

    #[tokio::test]
    async fn all_v3_routes_respond_not_implemented_or_ok() {
        let state = Arc::new(base_state());
        let app = proxy::build_router(state);

        let routes: &[(&str, StatusCode)] = &[
            ("/healthz", StatusCode::OK),
            ("/readyz", StatusCode::OK),
            ("/dashboard", StatusCode::OK),
            ("/metrics", StatusCode::OK),
            ("/.well-known/agent.json", StatusCode::OK),
            ("/sessions", StatusCode::OK),
            ("/supervisor/status", StatusCode::OK),
            ("/a2a/tasks", StatusCode::METHOD_NOT_ALLOWED),
            ("/events", StatusCode::OK),
        ];

        for (path, expected) in routes {
            let req = Request::builder().uri(*path).body(Body::empty()).unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                *expected,
                "{}: expected {:?}, got {:?}",
                path,
                expected,
                resp.status()
            );
        }
    }

    // -- 10. A2A task store - multi-node federation ready --

    #[tokio::test]
    async fn task_store_crud() {
        let store = a2a::TaskStore::new();

        let task = store.create("task-e2e".into());
        assert_eq!(task.id, "task-e2e");
        assert_eq!(task.status, a2a::TaskStatus::Submitted);

        let found = store.get("task-e2e");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "task-e2e");

        assert!(store.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn agent_registry_register_deregister() {
        use portail::a2a::{AgentCapabilities, AgentCard, registry::RegisterRequest};

        let registry = a2a::registry::AgentRegistry::new();

        let card = AgentCard {
            name: "test-agent".into(),
            description: "Test agent".into(),
            url: "http://localhost:9000".into(),
            version: "1.0.0".into(),
            capabilities: AgentCapabilities::default(),
            skills: vec![],
            authentication: None,
        };

        registry.register(RegisterRequest {
            id: "test-agent".into(),
            card,
            url: "http://localhost:9000".into(),
        });
        assert_eq!(registry.list().len(), 1);

        let found = registry.get("test-agent");
        assert!(found.is_some());
        assert_eq!(found.unwrap().card.name, "test-agent");

        assert!(registry.deregister("test-agent"));
        assert_eq!(registry.list().len(), 0);
    }

    // -- 11. Config - v3.0 feature flags --

    #[test]
    fn config_parses_ai_gateway_with_target_routing() {
        let toml_str = r#"
[ai_gateway]
enabled = true
upstream = "https://api.openai.com"
default_provider = "openai"

[[targets]]
name = "my-target"
base_url = "https://api.openai.com"
provider = "openai"
models = ["gpt-4", "gpt-3.5-turbo"]

[rate_limit]
enabled = false

[store]

[telemetry]
"#;
        let cfg: config::Config = toml::from_str(toml_str).unwrap();
        assert!(cfg.ai_gateway.as_ref().unwrap().enabled);
        assert_eq!(
            cfg.ai_gateway.as_ref().unwrap().upstream,
            "https://api.openai.com"
        );
        assert_eq!(
            cfg.ai_gateway.as_ref().unwrap().default_provider.as_deref(),
            Some("openai")
        );
        assert!(!cfg.targets.is_empty());
    }
}
