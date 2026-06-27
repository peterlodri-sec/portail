//! DoH resolver — DNS-over-HTTPS via Cloudflare/Google/Quad9.
//!
//! Default backend. Uses reqwest for HTTP/2 DoH queries.
//! Falls back through a chain of resolvers on failure.

use std::net::IpAddr;
use std::str::FromStr;

use super::{DnsRecordType, DnsResolver};

#[derive(Clone)]
pub struct DohResolver {
    client: reqwest::Client,
    urls: Vec<String>,
}

impl Default for DohResolver {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            urls: vec![
                "https://cloudflare-dns.com/dns-query".into(),
                "https://dns.google/resolve".into(),
                "https://dns.quad9.net/dns-query".into(),
            ],
        }
    }
}

impl DohResolver {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            urls,
        }
    }

    async fn try_resolve(
        &self,
        url: &str,
        name: &str,
        record_type: &str,
    ) -> Result<Vec<IpAddr>, String> {
        let resp = self
            .client
            .get(url)
            .header("accept", "application/dns-json")
            .query(&[("name", name), ("type", record_type)])
            .send()
            .await
            .map_err(|e| format!("DoH request failed: {}", e))?;

        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let answers = body["Answer"]
            .as_array()
            .ok_or("No Answer in DoH response")?;

        let mut ips = Vec::new();
        for answer in answers {
            if let Some(data) = answer["data"].as_str() {
                if let Ok(ip) = IpAddr::from_str(data) {
                    ips.push(ip);
                }
            }
        }
        Ok(ips)
    }
}

#[async_trait::async_trait]
impl DnsResolver for DohResolver {
    async fn resolve(&self, name: &str, record_type: DnsRecordType) -> Result<Vec<IpAddr>, String> {
        let rt = match record_type {
            DnsRecordType::A => "A",
            DnsRecordType::AAAA => "AAAA",
            DnsRecordType::PTR => "PTR",
        };

        let mut last_err = String::new();
        for url in &self.urls {
            match self.try_resolve(url, name, rt).await {
                Ok(ips) if !ips.is_empty() => return Ok(ips),
                Ok(_) => continue,
                Err(e) => {
                    last_err = e;
                    continue;
                }
            }
        }
        Err(format!(
            "All DoH resolvers failed. Last error: {}",
            last_err
        ))
    }

    fn name(&self) -> &'static str {
        "doh"
    }
}
