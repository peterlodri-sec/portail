mod cache;
mod purge;

pub use cache::CacheManager;
pub use cache::stats_logger;
pub use purge::purge_loop;

use axum::body::Bytes;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::sync::OnceLock;
use tracing::{debug, warn};

static CDN_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    CDN_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build CDN HTTP client")
    })
}

pub async fn handle(req: Request, cache: Arc<CacheManager>, origin: Option<String>) -> Response {
    let range = req
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("bytes=0-");
    let cache_key = format!("cdn:{}:{}", req.uri().path(), range);

    if let Some(entry) = cache.get(&cache_key).await {
        debug!(%cache_key, "CDN cache HIT");
        return cdn_response(StatusCode::OK, "HIT", entry);
    }

    let Some(origin) = origin else {
        return (StatusCode::NOT_FOUND, "no origin configured").into_response();
    };

    let origin_url = format!("{}{}", origin.trim_end_matches('/'), req.uri().path());
    let origin_req = client().get(&origin_url).header("range", range);

    match origin_req.send().await {
        Ok(resp) if resp.status().is_success() || resp.status() == StatusCode::PARTIAL_CONTENT => {
            let body = resp.bytes().await.unwrap_or_default();
            cache.put(&cache_key, body.clone()).await;
            debug!(%cache_key, "CDN cache MISS");
            cdn_response(StatusCode::OK, "MISS", body)
        }
        Ok(resp) => {
            warn!(%origin_url, status = %resp.status(), "CDN origin fetch failed");
            (resp.status(), "origin error").into_response()
        }
        Err(e) => {
            warn!(%origin_url, error = %e, "CDN origin unreachable");
            (StatusCode::BAD_GATEWAY, "origin unreachable").into_response()
        }
    }
}

fn cdn_response(status: StatusCode, cache_status: &'static str, body: Bytes) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert("x-cache-status", HeaderValue::from_static(cache_status));
    headers.insert("content-type", HeaderValue::from_static("application/octet-stream"));
    headers.insert("content-length", HeaderValue::from_str(&body.len().to_string()).unwrap());
    (status, headers, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_response_headers() {
        let resp = cdn_response(StatusCode::OK, "HIT", Bytes::from("data"));
        let (parts, _body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(
            parts.headers.get("x-cache-status").unwrap().to_str().unwrap(),
            "HIT"
        );
        assert_eq!(parts.headers.get("content-length").unwrap(), "4");
    }
}
