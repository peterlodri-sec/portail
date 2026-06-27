//! Fuzz Route — crash-test all routes with malformed input.

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzConfig {
    pub target_url: String,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            target_url: "http://localhost:8787".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzReport {
    pub generated_at: String,
    pub total_probes: usize,
    pub passed: usize,
    pub crashed: usize,
    pub errors: Vec<String>,
}

pub async fn run_fuzz_route(config: &FuzzConfig) -> FuzzReport {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let routes = ["/healthz", "/metrics", "/events"];
    let payloads = [
        serde_json::json!({"malformed": true}),
        serde_json::json!([1, 2, 3]),
        serde_json::json!("just a string"),
    ];

    let mut errors = Vec::new();
    let mut passed = 0usize;

    for route in &routes {
        for payload in &payloads {
            let url = format!("{}{}", config.target_url, route);
            match client.post(&url).json(payload).send().await {
                Ok(resp) => {
                    let s = resp.status().as_u16();
                    if s == 500 {
                        errors.push(format!("{route}: 500 on {payload}"));
                    } else {
                        passed += 1;
                    }
                }
                Err(e) => errors.push(format!("{route}: crash {e}")),
            }
        }
    }

    FuzzReport {
        generated_at: Utc::now().to_rfc3339(),
        total_probes: routes.len() * payloads.len(),
        passed,
        crashed: errors.len(),
        errors,
    }
}
