use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

// ── DNS Configuration ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub enabled: bool,
    pub listen: String,
    pub upstream: Vec<String>,
    pub doh_enabled: bool,
    pub doh_endpoints: Vec<String>,
    pub unbound_enabled: bool,
    pub unbound_config: Option<String>,
    pub blocklists: Vec<String>,
    pub allowlists: Vec<String>,
    pub hooks: Vec<DnsHook>,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: "127.0.0.1:53".into(),
            upstream: vec!["1.1.1.1".into(), "8.8.8.8".into()],
            doh_enabled: true,
            doh_endpoints: vec![
                "https://cloudflare-dns.com/dns-query".into(),
                "https://dns.google/dns-query".into(),
            ],
            unbound_enabled: false,
            unbound_config: None,
            blocklists: Vec::new(),
            allowlists: Vec::new(),
            hooks: Vec::new(),
        }
    }
}

// ── DNS Hooks ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsHook {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub action: DnsHookAction,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsHookAction {
    Block,
    Allow,
    Redirect(String),
    Log,
    Rewrite(String),
}

// ── DNS Record Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsQuery {
    pub name: String,
    pub record_type: DnsRecordType,
    pub source: IpAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsRecordType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    NS,
    SOA,
}

impl FromStr for DnsRecordType {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "A" => Ok(Self::A),
            "AAAA" => Ok(Self::AAAA),
            "CNAME" => Ok(Self::CNAME),
            "MX" => Ok(Self::MX),
            "TXT" => Ok(Self::TXT),
            "NS" => Ok(Self::NS),
            "SOA" => Ok(Self::SOA),
            _ => Err(format!("Unknown record type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsResponse {
    pub answers: Vec<DnsAnswer>,
    pub ttl: u32,
    pub authoritative: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsAnswer {
    pub name: String,
    pub record_type: DnsRecordType,
    pub data: String,
    pub ttl: u32,
}

// ── DNS Store (in-memory) ────────────────────────────────────────

pub struct DnsStore {
    records: std::sync::RwLock<HashMap<String, Vec<DnsAnswer>>>,
    hooks: std::sync::RwLock<Vec<DnsHook>>,
}

impl Default for DnsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsStore {
    pub fn new() -> Self {
        Self {
            records: std::sync::RwLock::new(HashMap::new()),
            hooks: std::sync::RwLock::new(Vec::new()),
        }
    }
    
    pub fn add_record(&self, name: String, answer: DnsAnswer) {
        let mut records = self.records.write().unwrap();
        records.entry(name).or_default().push(answer);
    }
    
    pub fn query(&self, name: &str, record_type: DnsRecordType) -> Vec<DnsAnswer> {
        let records = self.records.read().unwrap();
        records
            .get(name)
            .map(|answers| {
                answers
                    .iter()
                    .filter(|a| std::mem::discriminant(&a.record_type) == std::mem::discriminant(&record_type))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
    
    pub fn add_hook(&self, hook: DnsHook) {
        self.hooks.write().unwrap().push(hook);
    }
    
    pub fn remove_hook(&self, id: &str) -> bool {
        let mut hooks = self.hooks.write().unwrap();
        let pos = hooks.iter().position(|h| h.id == id);
        if let Some(p) = pos {
            hooks.remove(p);
            true
        } else {
            false
        }
    }
    
    pub fn apply_hooks(&self, query: &DnsQuery) -> Option<DnsHookAction> {
        let hooks = self.hooks.read().unwrap();
        for hook in hooks.iter().filter(|h| h.enabled) {
            if query.name.contains(&hook.pattern) || hook.pattern == "*" {
                return Some(hook.action.clone());
            }
        }
        None
    }
}

// ── DoH Client ───────────────────────────────────────────────────

pub struct DohClient {
    endpoints: Vec<String>,
    client: reqwest::Client,
}

impl DohClient {
    pub fn new(endpoints: Vec<String>) -> Self {
        Self {
            endpoints,
            client: reqwest::Client::new(),
        }
    }
    
    pub async fn query(&self, name: &str, record_type: DnsRecordType) -> Result<DnsResponse, String> {
        let qtype = match record_type {
            DnsRecordType::A => 1,
            DnsRecordType::AAAA => 28,
            DnsRecordType::CNAME => 5,
            DnsRecordType::MX => 15,
            DnsRecordType::TXT => 16,
            DnsRecordType::NS => 2,
            DnsRecordType::SOA => 6,
        };
        
        let url = format!(
            "{}?name={}&type={}",
            self.endpoints.first().unwrap_or(&"https://cloudflare-dns.com/dns-query".into()),
            name,
            qtype
        );
        
        let response = self.client
            .get(&url)
            .header("Accept", "application/dns-json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        
        let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        
        let answers = json["Answer"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|a| DnsAnswer {
                        name: a["name"].as_str().unwrap_or("").to_string(),
                        record_type: DnsRecordType::from_str(
                            &a["type"].as_u64().unwrap_or(1).to_string()
                        ).unwrap_or(DnsRecordType::A),
                        data: a["data"].as_str().unwrap_or("").to_string(),
                        ttl: a["TTL"].as_u64().unwrap_or(300) as u32,
                    })
                    .collect()
            })
            .unwrap_or_default();
        
        Ok(DnsResponse {
            answers,
            ttl: 300,
            authoritative: false,
        })
    }
}

// ── Network Isolation ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIsolation {
    pub enabled: bool,
    pub allowed_domains: Vec<String>,
    pub blocked_domains: Vec<String>,
    pub allowed_ips: Vec<IpAddr>,
    pub blocked_ips: Vec<IpAddr>,
    pub dns_only: bool,
}

impl Default for NetworkIsolation {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
            allowed_ips: Vec::new(),
            blocked_ips: Vec::new(),
            dns_only: true,
        }
    }
}

impl NetworkIsolation {
    pub fn is_allowed(&self, domain: &str, ip: Option<IpAddr>) -> bool {
        if !self.enabled {
            return true;
        }
        
        // Check blocked domains
        if self.blocked_domains.iter().any(|d| domain.contains(d.as_str())) {
            return false;
        }
        
        // Check blocked IPs
        if let Some(ip) = ip {
            if self.blocked_ips.contains(&ip) {
                return false;
            }
        }
        
        // If allowlists are set, only allow listed items
        if !self.allowed_domains.is_empty() {
            return self.allowed_domains.iter().any(|d| domain.contains(d.as_str()));
        }
        
        if !self.allowed_ips.is_empty() {
            if let Some(ip) = ip {
                return self.allowed_ips.contains(&ip);
            }
            return false;
        }
        
        true
    }
}

// ── HTTP Handlers ────────────────────────────────────────────────

pub async fn handle_dns_query(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    axum::Json(query): axum::Json<DnsQuery>,
) -> axum::Json<DnsResponse> {
    // Apply hooks
    if let Some(action) = state.dns_store.apply_hooks(&query) {
        match action {
            DnsHookAction::Block => {
                return axum::Json(DnsResponse {
                    answers: vec![],
                    ttl: 0,
                    authoritative: false,
                });
            }
            DnsHookAction::Redirect(target) => {
                return axum::Json(DnsResponse {
                    answers: vec![DnsAnswer {
                        name: query.name.clone(),
                        record_type: DnsRecordType::A,
                        data: target,
                        ttl: 300,
                    }],
                    ttl: 300,
                    authoritative: false,
                });
            }
            _ => {}
        }
    }
    
    // Query local store first
    let answers = state.dns_store.query(&query.name, query.record_type.clone());
    if !answers.is_empty() {
        return axum::Json(DnsResponse {
            answers,
            ttl: 300,
            authoritative: false,
        });
    }
    
    // Forward to DoH if enabled
    if let Some(ref doh) = state.doh_client {
        match doh.query(&query.name, query.record_type).await {
            Ok(response) => return axum::Json(response),
            Err(_) => {}
        }
    }
    
    axum::Json(DnsResponse {
        answers: vec![],
        ttl: 0,
        authoritative: false,
    })
}

// ── Module-level router ──────────────────────────────────────────

pub fn router() -> axum::Router<Arc<crate::AppState>> {
    axum::Router::new()
        .route("/dns/query", axum::routing::post(handle_dns_query))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn dns_store_add_query() {
        let store = DnsStore::new();
        store.add_record("example.com".into(), DnsAnswer {
            name: "example.com".into(),
            record_type: DnsRecordType::A,
            data: "1.2.3.4".into(),
            ttl: 300,
        });
        
        let answers = store.query("example.com", DnsRecordType::A);
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].data, "1.2.3.4");
    }
    
    #[test]
    fn dns_store_hooks() {
        let store = DnsStore::new();
        store.add_hook(DnsHook {
            id: "h1".into(),
            name: "block ads".into(),
            pattern: "ads.example.com".into(),
            action: DnsHookAction::Block,
            enabled: true,
        });
        
        let query = DnsQuery {
            name: "ads.example.com".into(),
            record_type: DnsRecordType::A,
            source: "127.0.0.1".parse().unwrap(),
        };
        
        let action = store.apply_hooks(&query);
        assert!(matches!(action, Some(DnsHookAction::Block)));
    }
    
    #[test]
    fn network_isolation() {
        let iso = NetworkIsolation {
            enabled: true,
            allowed_domains: vec!["example.com".into()],
            blocked_domains: vec!["evil.com".into()],
            ..Default::default()
        };
        
        assert!(iso.is_allowed("api.example.com", None));
        assert!(!iso.is_allowed("evil.com", None));
        assert!(!iso.is_allowed("unknown.com", None));
    }
}
