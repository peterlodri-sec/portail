use std::collections::HashMap;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

criterion_group!(benches, bench_mcp_encode, bench_cache_ops, bench_header_strip, bench_config_parse, bench_rate_limit, bench_bounded_meta);
criterion_main!(benches);

fn bench_mcp_encode(c: &mut Criterion) {
    let mut headers = HashMap::new();
    headers.insert("content-type".into(), "application/json".into());
    headers.insert("authorization".into(), "Bearer sk-test-1234".into());

    c.bench_function("mcp_encode_frame", |b| {
        b.iter(|| {
            black_box(portail::mcp::encode_frame(
                black_box("POST"),
                black_box("/mcp/tools/call"),
                black_box(headers.clone()),
                black_box(b"{\"name\":\"test\"}"),
            ))
        })
    });

    c.bench_function("mcp_encode_small", |b| {
        let empty = HashMap::new();
        b.iter(|| {
            black_box(portail::mcp::encode_frame(
                black_box("GET"),
                black_box("/mcp/tools/list"),
                black_box(empty.clone()),
                black_box(b""),
            ))
        })
    });
}

fn bench_cache_ops(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let cache = rt.block_on(async {
        let cfg = portail::config::CdnConfig {
            enabled: true,
            origin: "http://127.0.0.1:9000".into(),
            cache_dir: "/tmp/_cdn_bench".into(),
            cache_size: "100m".into(),
            nats_url: None,
            domains: vec![],
        };
        portail::cdn::CacheManager::new(&cfg)
    });

    c.bench_function("cache_put", |b| {
        let data = vec![0u8; 4096];
        let body = bytes::Bytes::from(data);
        b.to_async(&rt).iter(|| {
            let c = cache.clone();
            let b = body.clone();
            async move { c.put(black_box("bench:key"), black_box(b)).await }
        })
    });
}

fn bench_header_strip(c: &mut Criterion) {
    use axum::http::{HeaderMap, HeaderValue, HeaderName};

    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("example.com"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("authorization", HeaderValue::from_static("Bearer test"));
    headers.insert("x-custom", HeaderValue::from_static("value"));
    headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
    headers.insert("connection", HeaderValue::from_static("keep-alive"));
    headers.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
    headers.insert(HeaderName::from_static("x-request-id"), HeaderValue::from_static("abc-123"));

    c.bench_function("gateway_strip_hop_by_hop", |b| {
        b.iter(|| {
            black_box(portail::gateway::strip_hop_by_hop(black_box(&headers)))
        })
    });
}

// ── v2.0 benchmarks ──────────────────────────────────────────────

fn bench_config_parse(c: &mut Criterion) {
    let toml_str = r#"
listen = "0.0.0.0:8787"
[rate_limit]
enabled = true
burst = 30
per_second = 10.0
[auth]
enabled = false
[store]
enabled = true
provider = "sqlite"
db_path = "/var/lib/portail/events.db"
retention_days = 30
[telemetry]
enabled = false
"#;
    c.bench_function("config_parse", |b| {
        b.iter(|| {
            let _: portail::config::Config = toml::from_str(black_box(toml_str)).unwrap();
        })
    });
}

fn bench_rate_limit(c: &mut Criterion) {
    let limiter = portail::rate_limit::RateLimiter::new(portail::rate_limit::RateLimitConfig {
        enabled: true,
        burst: 30,
        per_second: 1000.0,
        ..Default::default()
    });
    c.bench_function("rate_limit_check", |b| {
        b.iter(|| {
            let _ = limiter.check(black_box("api-key-123"), black_box("/v1/chat/completions"));
        })
    });
}

fn bench_bounded_meta(c: &mut Criterion) {
    c.bench_function("bounded_meta_insert_16", |b| {
        b.iter(|| {
            let mut m = portail::types::BoundedMeta::new();
            for i in 0..16 {
                let _ = m.insert(format!("key{}", i), "value".into());
            }
            black_box(m)
        })
    });

    c.bench_function("bounded_meta_json_roundtrip", |b| {
        let mut m = portail::types::BoundedMeta::new();
        for i in 0..8 {
            let _ = m.insert(format!("key{}", i), "value".into());
        }
        let json = serde_json::to_string(&m).unwrap();
        b.iter(|| {
            let _: portail::types::BoundedMeta = serde_json::from_str(black_box(&json)).unwrap();
        })
    });
}
