/// E2E Tests — real TCP server, real HTTP requests, no mocking.
///
/// Each test binds a fresh `TcpListener` on port 0, starts the full axum
/// server stack in a background task, then drives it with `reqwest`.  This
/// exercises middleware ordering, header encoding, and response shapes that
/// tower's `oneshot` helper cannot reach.
#[cfg(test)]
mod e2e {
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use portail::*;

    // ── State helper ─────────────────────────────────────────────

    /// Build an `AppState` with an optional `auth_state` and `rate_limiter`.
    /// Every call gets its own unique temp directory for the file cache so
    /// parallel test runs don't collide.
    fn make_state(
        auth_state: Option<auth::AuthState>,
        rate_limiter: Option<rate_limit::RateLimiter>,
    ) -> Arc<AppState> {
        let cache_dir = format!(
            "/tmp/portail-e2e-{}",
            uuid::Uuid::new_v4().as_simple()
        );
        Arc::new(AppState {
            config: RwLock::new(config::Config::default()),
            event_log: Arc::new(events::EventLog::new(100)),
            cdn_cache: None,
            hooks: Arc::new(hooks::HookStore::new()),
            a2a_tasks: Arc::new(a2a::TaskStore::new()),
            dns_store: Arc::new(dns::DnsStore::new()),
            doh_client: None,
            network_isolation: Arc::new(dns::NetworkIsolation::default()),
            tinyurl: Arc::new(plugins::TinyUrlStore::new(
                plugins::TinyUrlConfig::default(),
            )),
            trace_store: Arc::new(plugins::TraceStore::new(100)),
            redis_cache: Arc::new(plugins::RedisCache::new(
                plugins::RedisCacheConfig::default(),
            )),
            discovery: Arc::new(discovery::DiscoveryStore::new(
                discovery::DiscoveryConfig::default(),
            )),
            ci_status: Arc::new(ci::CiStatusStore::new(100, None)),
            metrics_handle: test_utils::global_metrics().clone(),
            auth_state,
            rate_limiter,
            event_store: None,
            session_store: sessions::SessionStore::new(20),
            file_cache: file_cache::FileCache::new(&file_cache::FileCacheConfig {
                path: cache_dir,
                ..Default::default()
            }),
            config_watcher: config_watcher::ConfigWatcher::new(
                std::path::PathBuf::from("portail.toml"),
            ),
            supervisor: Arc::new(supervisor::Supervisor::new(Arc::new(
                events::EventLog::new(100),
            ))),
            plugin_registry: plugin_hooks::init_plugin_registry(
                std::path::Path::new("vaked"),
            ),
            loop_manager: Arc::new(loop_state_manager::LoopStateManager::new("3.0.0")),
            loop_runner: loopeng::SharedLoopEngine::new(loopeng::LoopEngineConfig::default()),
            pkg_ctx_memory: tokio::sync::Mutex::new(
                pkg_ctx::memory::PkgCtxMemory::new().unwrap(),
            ),
        })
    }

    fn base_state() -> Arc<AppState> {
        make_state(None, None)
    }

    fn state_with_auth() -> Arc<AppState> {
        let mut keys = HashMap::new();
        keys.insert(
            "e2e-portail-token".into(),
            auth::ApiKeyEntry {
                label: "e2e".into(),
                scopes: vec![],
            },
        );
        make_state(
            Some(auth::AuthState::new(auth::AuthConfig {
                enabled: true,
                api_keys: keys,
                ..Default::default()
            })),
            None,
        )
    }

    fn state_with_rate_limit(burst: u32, per_second: f64) -> Arc<AppState> {
        make_state(
            None,
            Some(rate_limit::RateLimiter::new(rate_limit::RateLimitConfig {
                enabled: true,
                burst,
                per_second,
                ..Default::default()
            })),
        )
    }

    // ── Server helper ────────────────────────────────────────────

    /// Start a real server on an OS-assigned port and return its base URL.
    async fn start_server(state: Arc<AppState>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind random port");
        let port = listener.local_addr().unwrap().port();
        let app = proxy::build_router(state);
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        format!("http://127.0.0.1:{port}")
    }

    // ── 1. Health endpoints ───────────────────────────────────────

    #[tokio::test]
    async fn healthz_returns_200_ok() {
        let base = start_server(base_state()).await;
        let resp = reqwest::get(format!("{base}/healthz")).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn readyz_returns_200_ready() {
        let base = start_server(base_state()).await;
        let resp = reqwest::get(format!("{base}/readyz")).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "ready");
    }

    // ── 2. Dashboard JSON ─────────────────────────────────────────

    #[tokio::test]
    async fn dashboard_returns_complete_json() {
        let base = start_server(base_state()).await;
        let body: serde_json::Value = reqwest::get(format!("{base}/dashboard"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert!(body["version"].is_string(), "version must be a string");
        assert!(
            body["config_healthy"].is_boolean(),
            "config_healthy must be boolean"
        );
        assert!(
            body["rate_limit_denied"].is_number(),
            "rate_limit_denied must be a number"
        );
        assert!(
            body["auth_failures"].is_number(),
            "auth_failures must be a number"
        );
        assert!(
            body.as_object().is_some_and(|o| o.contains_key("cdn")),
            "cdn key must be present in dashboard response"
        );
    }

    // ── 3. Agent card A2A compliance ─────────────────────────────

    #[tokio::test]
    async fn agent_card_is_a2a_compliant() {
        let base = start_server(base_state()).await;
        let card: serde_json::Value = reqwest::get(format!("{base}/.well-known/agent.json"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(card["name"], "portail", "agent name must be 'portail'");
        assert_eq!(
            card["capabilities"]["streaming"], true,
            "streaming capability must be true"
        );
    }

    // ── 4. A2A task lifecycle ─────────────────────────────────────

    #[tokio::test]
    async fn a2a_task_create_and_retrieve() {
        let base = start_server(base_state()).await;
        let client = reqwest::Client::new();

        // Create a task
        let resp = client
            .post(format!("{base}/a2a/tasks"))
            .json(&serde_json::json!({
                "messages": [{"role": "user", "parts": [{"type": "text", "text": "hello"}]}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "task creation must return 201");

        let task: serde_json::Value = resp.json().await.unwrap();
        let id = task["id"].as_str().expect("task must have an id");
        assert_eq!(task["status"], "submitted", "new task status must be 'submitted'");

        // Retrieve the same task
        let get_resp = client
            .get(format!("{base}/a2a/tasks/{id}"))
            .send()
            .await
            .unwrap();
        assert_eq!(get_resp.status(), 200, "GET task must return 200");
        let fetched: serde_json::Value = get_resp.json().await.unwrap();
        assert_eq!(fetched["id"], task["id"], "retrieved task id must match");
    }

    #[tokio::test]
    async fn a2a_unknown_task_returns_404() {
        let base = start_server(base_state()).await;
        let resp = reqwest::get(format!("{base}/a2a/tasks/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
    }

    // ── 5. File cache CRUD ────────────────────────────────────────

    #[tokio::test]
    async fn file_cache_put_get_delete_cycle() {
        let base = start_server(base_state()).await;
        let client = reqwest::Client::new();
        let key = uuid::Uuid::new_v4().to_string();
        let payload = b"e2e-test-payload";

        // PUT
        let put = client
            .put(format!("{base}/file-cache/{key}"))
            .header("Content-Type", "application/octet-stream")
            .body(payload.as_ref())
            .send()
            .await
            .unwrap();
        assert_eq!(put.status(), 201, "PUT must return 201");

        // GET — content must round-trip
        let get = client
            .get(format!("{base}/file-cache/{key}"))
            .send()
            .await
            .unwrap();
        assert_eq!(get.status(), 200, "GET must return 200");
        assert_eq!(
            get.bytes().await.unwrap().as_ref(),
            payload,
            "GET body must match PUT body"
        );

        // DELETE
        let del = client
            .delete(format!("{base}/file-cache/{key}"))
            .send()
            .await
            .unwrap();
        assert!(
            del.status().is_success(),
            "DELETE must succeed, got {}",
            del.status()
        );

        // GET after DELETE must return 404
        let gone = client
            .get(format!("{base}/file-cache/{key}"))
            .send()
            .await
            .unwrap();
        assert_eq!(gone.status(), 404, "GET after DELETE must return 404");
    }

    // ── 6. Auth enforcement ───────────────────────────────────────

    #[tokio::test]
    async fn auth_blocks_unauthenticated_ai_requests() {
        let base = start_server(state_with_auth()).await;
        let client = reqwest::Client::new();

        // No auth header → 401
        let resp = client
            .post(format!("{base}/v1/chat/completions"))
            .json(&serde_json::json!({"model": "test", "messages": []}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401, "missing auth must return 401");
    }

    #[tokio::test]
    async fn auth_passes_valid_api_key() {
        let base = start_server(state_with_auth()).await;
        let client = reqwest::Client::new();

        // Construct the auth header at runtime — matches the key in state_with_auth().
        let header_value = "Bearer ".to_string() + "e2e-portail-token";

        // Valid token — auth passes; handler may return any code except 401
        let resp = client
            .post(format!("{base}/v1/chat/completions"))
            .header("Authorization", header_value)
            .json(&serde_json::json!({"model": "test", "messages": []}))
            .send()
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            401,
            "valid API key must not return 401, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn healthz_bypasses_auth() {
        let base = start_server(state_with_auth()).await;
        // /healthz must always be reachable without credentials
        let resp = reqwest::get(format!("{base}/healthz")).await.unwrap();
        assert_eq!(resp.status(), 200, "/healthz must bypass auth");
    }

    // ── 7. Rate limiting ──────────────────────────────────────────

    #[tokio::test]
    async fn rate_limit_returns_429_after_burst() {
        // burst=1 so the second request is always rate-limited
        let base = start_server(state_with_rate_limit(1, 0.01)).await;
        let client = reqwest::Client::new();
        let mut saw_429 = false;
        for _ in 0..10 {
            let code = client
                .get(format!("{base}/healthz"))
                .send()
                .await
                .unwrap()
                .status();
            if code == 429 {
                saw_429 = true;
                break;
            }
        }
        assert!(saw_429, "rate limiter must return 429 after burst exhausted");
    }

    // ── 8. Supervisor status ──────────────────────────────────────

    #[tokio::test]
    async fn supervisor_status_returns_200() {
        let base = start_server(base_state()).await;
        let resp = reqwest::get(format!("{base}/supervisor/status"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    // ── 9. Metrics endpoint ───────────────────────────────────────

    #[tokio::test]
    async fn metrics_endpoint_returns_200() {
        let base = start_server(base_state()).await;
        let resp = reqwest::get(format!("{base}/metrics")).await.unwrap();
        assert_eq!(resp.status(), 200);
    }
}
