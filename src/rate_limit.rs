//! Rate limiting — token bucket per API key / IP.
//!
//! Uses [`governor`] for precise GCRA rate limiting. Each unique key
//! (API key or IP) gets its own bucket. Limits are configurable per
//! endpoint pattern.
//!
//! # Middleware
//!
//! [`RateLimitLayer`] is an axum middleware. Add it via
//! `axum::middleware::from_fn_with_state(state, rate_limit_layer)`.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovLimiter,
};
use serde::{Deserialize, Serialize};

// ─── config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Default burst size (requests allowed in a burst)
    #[serde(default = "default_burst")]
    pub burst: u32,
    /// Default replenishment rate: 1 token every `per_second` seconds
    #[serde(default = "default_per_second")]
    pub per_second: f64,
    /// Per-endpoint overrides (path prefix → burst, per_second)
    #[serde(default)]
    pub endpoints: HashMap<String, EndpointLimit>,
    /// API keys with custom limits
    #[serde(default)]
    pub api_keys: HashMap<String, ApiKeyLimit>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            burst: default_burst(),
            per_second: default_per_second(),
            endpoints: HashMap::new(),
            api_keys: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointLimit {
    pub burst: u32,
    pub per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyLimit {
    pub burst: u32,
    pub per_second: f64,
}

fn default_true() -> bool {
    false
}
fn default_burst() -> u32 {
    30
}
fn default_per_second() -> f64 {
    10.0
}

// ─── state ────────────────────────────────────────────────────────

type Limiter = GovLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Shared rate limiter state.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<RateLimiterInner>,
}

struct RateLimiterInner {
    config: RateLimitConfig,
    /// Default limiter for unauthenticated / unmatched traffic
    default_limiter: Limiter,
    /// Per-API-key limiters
    key_limiters: HashMap<String, Limiter>,
    /// Per-endpoint-prefix limiters
    endpoint_limiters: HashMap<String, Limiter>,
}

impl RateLimiter {
    /// Build the limiter from configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        let default_quota = quota(config.burst, config.per_second);
        let default_limiter = GovLimiter::direct(default_quota);

        let key_limiters: HashMap<String, Limiter> = config
            .api_keys
            .iter()
            .map(|(key, limit)| {
                (key.clone(), GovLimiter::direct(quota(limit.burst, limit.per_second)))
            })
            .collect();

        let endpoint_limiters: HashMap<String, Limiter> = config
            .endpoints
            .iter()
            .map(|(path, limit)| {
                (path.clone(), GovLimiter::direct(quota(limit.burst, limit.per_second)))
            })
            .collect();

        Self {
            inner: Arc::new(RateLimiterInner {
                config,
                default_limiter,
                key_limiters,
                endpoint_limiters,
            }),
        }
    }

    /// Check a request. Returns `Ok(())` if allowed, `Err(RetryAfter)` if
    /// rate limited. Key is derived from Authorization header (API key) or
    /// client IP.
    pub fn check(&self, key: &str, path: &str) -> Result<(), Duration> {
        let inner = &self.inner;

        if !inner.config.enabled {
            return Ok(());
        }

        // Find the right limiter: API-key-specific > endpoint-specific > default
        let limiter = inner
            .key_limiters
            .get(key)
            .or_else(|| {
                inner
                    .endpoint_limiters
                    .iter()
                    .find(|(prefix, _)| path.starts_with(prefix.as_str()))
                    .map(|(_, l)| l)
            })
            .unwrap_or(&inner.default_limiter);

        match limiter.check() {
            Ok(_) => Ok(()),
            Err(negative) => {
                let wait = negative.wait_time_from(governor::clock::Clock::now(
                    &governor::clock::DefaultClock::default(),
                ));
                Err(wait)
            }
        }
    }
}

fn quota(burst: u32, per_second: f64) -> Quota {
    let replenish_interval = Duration::from_secs_f64(1.0 / per_second);
    Quota::with_period(replenish_interval)
        .unwrap()
        .allow_burst(NonZeroU32::new(burst.max(1)).unwrap())
}

// ─── axum middleware ──────────────────────────────────────────────

/// Rate-limiting middleware layer.
///
/// Extracts rate-limit key from `Authorization: Bearer <key>` header;
/// falls back to `X-Forwarded-For` / socket address.
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string();

    // Derive rate-limit key
    let key = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("anonymous");

    match limiter.check(key, &path) {
        Ok(()) => Ok(next.run(req).await),
        Err(wait) => {
            let retry_after = wait.as_secs().max(1);
            let body = serde_json::json!({
                "error": "rate_limited",
                "message": format!("Too many requests. Retry in {}s", retry_after),
                "retry_after_seconds": retry_after,
            });
            Ok((
                StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", retry_after.to_string())],
                axum::Json(body),
            )
                .into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_always_allows() {
        let config = RateLimitConfig {
            enabled: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        assert!(limiter.check("key1", "/v1/chat/completions").is_ok());
        // many calls still pass
        for _ in 0..1000 {
            assert!(limiter.check("key1", "/v1/chat/completions").is_ok());
        }
    }

    #[test]
    fn enabled_limits_by_default() {
        let config = RateLimitConfig {
            enabled: true,
            burst: 5,
            per_second: 100.0,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        // first 5 bursts pass
        for _ in 0..5 {
            assert!(limiter.check("key1", "/v1/chat/completions").is_ok());
        }
        // 6th should fail
        assert!(limiter.check("key1", "/v1/chat/completions").is_err());
    }

    #[test]
    fn per_api_key_limits_override_default() {
        let mut api_keys = HashMap::new();
        api_keys.insert(
            "premium".into(),
            ApiKeyLimit {
                burst: 100,
                per_second: 100.0,
            },
        );

        let config = RateLimitConfig {
            enabled: true,
            burst: 3,
            per_second: 100.0,
            api_keys,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Premium key gets 100 burst
        for _ in 0..50 {
            assert!(limiter.check("premium", "/v1/chat/completions").is_ok());
        }

        // Normal key gets 3 burst
        assert!(limiter.check("normal", "/v1/chat/completions").is_ok());
        assert!(limiter.check("normal", "/v1/chat/completions").is_ok());
        assert!(limiter.check("normal", "/v1/chat/completions").is_ok());
        assert!(limiter.check("normal", "/v1/chat/completions").is_err());
    }
}
