//! Drift Detect — capture/replay traffic regression detection.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftConfig {
    pub target_url: String,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            target_url: "http://localhost:8787".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftResult {
    pub endpoint: String,
    pub status: u16,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub generated_at: String,
    pub results: HashMap<String, DriftResult>,
    pub total: usize,
    pub drifted: usize,
}

pub async fn run_drift_detect(config: &DriftConfig) -> DriftReport {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let endpoints = ["/healthz", "/metrics", "/events"];
    let mut results = HashMap::new();
    let mut drifted = 0usize;

    for ep in &endpoints {
        let url = format!("{}{}", config.target_url, ep);
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let ok = resp.status().is_success();
                if !ok {
                    drifted += 1;
                }
                results.insert(
                    ep.to_string(),
                    DriftResult {
                        endpoint: ep.to_string(),
                        status,
                        ok,
                        error: None,
                    },
                );
            }
            Err(e) => {
                drifted += 1;
                results.insert(
                    ep.to_string(),
                    DriftResult {
                        endpoint: ep.to_string(),
                        status: 0,
                        ok: false,
                        error: Some(e.to_string()),
                    },
                );
            }
        }
    }

    DriftReport {
        generated_at: Utc::now().to_rfc3339(),
        results,
        total: endpoints.len(),
        drifted,
    }
}
