use axum::extract::Request;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use metrics::counter;
use reqwest::Client;
use std::sync::OnceLock;
use tracing::{debug, warn};

const HOP_BY_HOP: &[&str] = &[
    "host",
    "connection",
    "transfer-encoding",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
];

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .http2_keep_alive_interval(Some(std::time::Duration::from_secs(30)))
            .build()
            .expect("failed to build HTTP client")
    })
}

pub fn strip_hop_by_hop(headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (key, value) in headers.iter() {
        if !HOP_BY_HOP.contains(&key.as_str()) {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

fn add_x_forwarded_for(headers: &mut HeaderMap) {
    match headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        Some(existing) => {
            headers.insert(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_str(&format!("{existing}, portail")).unwrap(),
            );
        }
        None => {
            headers.insert(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static("portail"),
            );
        }
    }
}

/// Forward a request to upstream. Accepts pre-read body bytes for hook injection.
pub async fn forward_with_body(
    upstream: &str,
    parts: axum::http::request::Parts,
    body_bytes: Bytes,
) -> Response {
    let uri = &parts.uri;
    let method = parts.method.clone();

    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let upstream_url = format!("{}{}", upstream.trim_end_matches('/'), path);

    let mut forward_headers = strip_hop_by_hop(&parts.headers);
    add_x_forwarded_for(&mut forward_headers);

    debug!(method = %method, %upstream_url, body_size = body_bytes.len(), "forwarding");

    let client = client();
    let req_builder = match method {
        axum::http::Method::GET => client.get(&upstream_url),
        axum::http::Method::POST => client.post(&upstream_url),
        axum::http::Method::PUT => client.put(&upstream_url),
        axum::http::Method::DELETE => client.delete(&upstream_url),
        axum::http::Method::PATCH => client.patch(&upstream_url),
        _ => client.get(&upstream_url),
    }
    .headers(forward_headers)
    .body(body_bytes);

    match req_builder.send().await {
        Ok(resp) => {
            counter!("ai_gateway_requests", "status" => resp.status().as_u16().to_string())
                .increment(1);
            let status = resp.status();
            let resp_headers = resp.headers().clone();
            let resp_body = resp.bytes().await.unwrap_or_default();

            let mut out_headers = strip_hop_by_hop(&resp_headers);
            out_headers.insert("x-portail-proxy", HeaderValue::from_static("ai-gateway"));
            (status, out_headers, resp_body).into_response()
        }
        Err(e) => {
            warn!(%upstream_url, error = %e, "upstream unreachable");
            counter!("ai_gateway_errors").increment(1);
            (StatusCode::BAD_GATEWAY, "upstream unavailable").into_response()
        }
    }
}

/// Forward a complete request to upstream.
pub async fn forward(upstream: &str, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 10_000_000).await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!(error = %e, "failed to read request body");
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body too large or unreadable",
            )
                .into_response();
        }
    };
    forward_with_body(upstream, parts, body_bytes).await
}

/// Forward a request to a specific path on the upstream with raw body bytes.
pub async fn forward_with_url(
    upstream: &str,
    path: &str,
    body: &[u8],
) -> Result<Response, reqwest::Error> {
    let url = format!("{}{}", upstream.trim_end_matches('/'), path);
    let resp = client()
        .post(&url)
        .header("content-type", "application/json")
        .body(body.to_vec())
        .send()
        .await?;
    counter!("a2c_requests").increment(1);
    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let resp_body = resp.bytes().await?;
    let mut out_headers = strip_hop_by_hop(&resp_headers);
    out_headers.insert("x-portail-proxy", HeaderValue::from_static("a2c"));
    Ok((status, out_headers, resp_body).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_by_hop_filtered() {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("example.com"));
        h.insert("content-type", HeaderValue::from_static("application/json"));
        h.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        h.insert("x-custom", HeaderValue::from_static("keep-me"));
        let out = strip_hop_by_hop(&h);
        assert!(out.get("host").is_none());
        assert!(out.get("transfer-encoding").is_none());
        assert_eq!(out.get("content-type").unwrap(), "application/json");
        assert_eq!(out.get("x-custom").unwrap(), "keep-me");
    }

    #[test]
    fn x_forwarded_for_appended() {
        let mut h = HeaderMap::new();
        add_x_forwarded_for(&mut h);
        assert_eq!(h.get("x-forwarded-for").unwrap(), "portail");

        h.insert(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_static("1.2.3.4"),
        );
        add_x_forwarded_for(&mut h);
        assert_eq!(h.get("x-forwarded-for").unwrap(), "1.2.3.4, portail");
    }
}
