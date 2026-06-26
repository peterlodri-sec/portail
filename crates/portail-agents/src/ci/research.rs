//! Deep Research CI Agent — multi-source search + bulk fetch.
//!
//! Phase 1: Query multiple search APIs in parallel (DuckDuckGo, BraveSearch,
//!          Linkup), collect URLs, deduplicate.
//! Phase 2: Bulk-fetch all collected pages (stubbed).
//! Phase 3: Structured research report (stubbed).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchConfig {
    pub max_sources_per_api: usize,
    pub brave_api_key: Option<String>,
    pub linkup_api_key: Option<String>,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            max_sources_per_api: 5,
            brave_api_key: None,
            linkup_api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String, // "duckduckgo", "brave", "linkup"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReport {
    pub query: String,
    pub generated_at: String,
    pub sources_found: usize,
    pub sources_fetched: usize,
    pub results: Vec<SearchResult>,
    pub errors: Vec<String>,
}

/// Phase 1: Parallel search across multiple engines
pub async fn search_all(config: &ResearchConfig, query: &str) -> (Vec<SearchResult>, Vec<String>) {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    // DuckDuckGo (no API key needed)
    match search_duckduckgo(query, config.max_sources_per_api).await {
        Ok(mut r) => results.append(&mut r),
        Err(e) => errors.push(format!("duckduckgo: {e}")),
    }

    // Brave Search (API key required)
    if let Some(ref key) = config.brave_api_key {
        match search_brave(query, key, config.max_sources_per_api).await {
            Ok(mut r) => results.append(&mut r),
            Err(e) => errors.push(format!("brave: {e}")),
        }
    }

    // Linkup (API key required)
    if let Some(ref key) = config.linkup_api_key {
        match search_linkup(query, key, config.max_sources_per_api).await {
            Ok(mut r) => results.append(&mut r),
            Err(e) => errors.push(format!("linkup: {e}")),
        }
    }

    // Deduplicate by URL
    let mut seen = HashSet::new();
    results.retain(|r| seen.insert(r.url.clone()));

    (results, errors)
}

async fn search_duckduckgo(query: &str, max: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&url)
        .header("User-Agent", "portail-research-agent/1.0")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body = resp.text().await.map_err(|e| e.to_string())?;

    // Minimal HTML parsing for result links
    let mut results = Vec::new();
    for link in body.split("<a rel=\"nofollow\" href=\"") {
        if results.len() >= max { break; }
        let href = link.split('\"').next().unwrap_or("");
        let after_href = link.split('>').nth(1).unwrap_or("");
        let title = after_href.split('<').next().unwrap_or("").trim();
        let snippet = link.split("<a class=\"result-snippet\">")
            .nth(1)
            .and_then(|s| s.split('<').next())
            .unwrap_or("")
            .trim()
            .to_string();

        if !href.is_empty() && !title.is_empty() {
            results.push(SearchResult {
                title: title.to_string(),
                url: href.to_string(),
                snippet,
                source: "duckduckgo".into(),
            });
        }
    }
    Ok(results)
}

async fn search_brave(query: &str, api_key: &str, max: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlencoding(query), max
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    if let Some(web) = json.get("web").and_then(|w| w.get("results")) {
        if let Some(arr) = web.as_array() {
            for item in arr.iter().take(max) {
                results.push(SearchResult {
                    title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    url: item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                    snippet: item.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                    source: "brave".into(),
                });
            }
        }
    }
    Ok(results)
}

async fn search_linkup(query: &str, api_key: &str, max: usize) -> Result<Vec<SearchResult>, String> {
    let url = "https://api.linkup.so/v1/search";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "q": query,
            "max_results": max,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    if let Some(arr) = json.as_array() {
        for item in arr.iter().take(max) {
            results.push(SearchResult {
                title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                url: item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                snippet: item.get("snippet").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                source: "linkup".into(),
            });
        }
    }
    Ok(results)
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

/// Phase 2: Bulk fetch all collected URLs (stub — returns empty content)
pub async fn bulk_fetch(urls: &[String]) -> Vec<(String, String, Vec<String>)> {
    // Phase 2 implementation: fetch all URLs in parallel with reqwest
    // For now: return stubs
    urls.iter().map(|url| (url.clone(), "[stub — fetch in Phase 2]".into(), vec![])).collect()
}

/// Run full research pipeline
pub async fn run_research(config: &ResearchConfig, query: &str) -> ResearchReport {
    let (results, errors) = search_all(config, query).await;
    let urls: Vec<String> = results.iter().map(|r| r.url.clone()).collect();
    let fetched = bulk_fetch(&urls).await;

    ResearchReport {
        query: query.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        sources_found: results.len(),
        sources_fetched: fetched.len(),
        results,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding("hello world"), "hello+world");
        assert_eq!(urlencoding("a/b"), "a%2Fb");
    }

    #[test]
    fn test_research_config_default() {
        let cfg = ResearchConfig::default();
        assert_eq!(cfg.max_sources_per_api, 5);
    }
}
