//! Property-based tests — proptest suite.
//!
//! # v2.0
//!
//! Tests invariants with random input generation.
//! Catches edge cases that handwritten tests miss.

#[cfg(test)]
mod proptests {
    use portail::rate_limit::RateLimiter;
    use proptest::prelude::*;

    // ── RateLimiter — never panics on any input ───────────────────

    proptest! {
        #[test]
        fn rate_limiter_never_panics(
            burst in 1u32..1000,
            per_second in 0.1f64..1000.0,
            key in "[a-zA-Z0-9_-]{1,64}",
            path in "/([a-z0-9/_-]{0,200})",
        ) {
            let config = portail::rate_limit::RateLimitConfig {
                enabled: true,
                burst,
                per_second,
                ..Default::default()
            };
            let limiter = RateLimiter::new(config);
            let result = limiter.check(&key, &path);
            // Must never panic — can be Ok or Err
            let _ = result;
        }

        #[test]
        fn rate_limiter_disabled_always_ok(
            burst in 0u32..100,
            per_second in 0.0f64..100.0,
        ) {
            let config = portail::rate_limit::RateLimitConfig {
                enabled: false,
                burst,
                per_second,
                ..Default::default()
            };
            let limiter = RateLimiter::new(config);
            for _ in 0..1000 {
                assert!(limiter.check("any", "/any").is_ok());
            }
        }
    }

    // ── portail::types::BoundedMeta — bounded under any input ─────────────────────

    proptest! {
        #[test]
        fn bounded_meta_never_grows_unbounded(
            keys in prop::collection::vec("[a-z]{1,16}", 0..50),
            vals in prop::collection::vec("[a-z]{1,32}", 0..50),
        ) {
            let mut m = portail::types::BoundedMeta::new();
            let n = keys.len().min(vals.len());
            for i in 0..n {
                let _ = m.insert(keys[i].clone(), vals[i].clone());
            }
            assert!(m.len() <= 16, "portail::types::BoundedMeta grew to {} entries", m.len());
        }

        #[test]
        fn bounded_meta_roundtrip_serializable(
            keys in prop::collection::vec("[a-z]{1,8}", 1..10),
            vals in prop::collection::vec("[a-zA-Z0-9]{1,64}", 1..10),
        ) {
            let mut m = portail::types::BoundedMeta::new();
            let n = keys.len().min(vals.len());
            for i in 0..n {
                let _ = m.insert(keys[i].clone(), vals[i].clone());
            }
            let json = serde_json::to_string(&m).unwrap();
            let rt: portail::types::BoundedMeta = serde_json::from_str(&json).unwrap();
            assert_eq!(rt.len(), m.len());
        }
    }

    // ── AuthState — valid/invalid inputs ──────────────────────────

    proptest! {
        #[test]
        fn auth_config_always_parses(
            enabled: bool,
            keys in prop::collection::vec("[a-zA-Z0-9]{8,64}", 0..10),
        ) {
            use std::collections::HashMap;
            let mut api_keys = HashMap::new();
            for k in &keys {
                api_keys.insert(k.clone(), portail::auth::ApiKeyEntry {
                    label: format!("key-{}", k),
                    scopes: vec![],
                });
            }
            let config = portail::auth::AuthConfig {
                enabled,
                api_keys,
                jwt: None,
                bypass_paths: vec!["/healthz".into(), "/metrics".into()],
            };
            let auth = portail::auth::AuthState::new(config);
            // AuthState must never panic on construction
            let _ = auth;
        }
    }
}
