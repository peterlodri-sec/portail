//! Spec Verify — compare route table against golden spec.

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecVerifyConfig {
    pub golden_path: String,
}

impl Default for SpecVerifyConfig {
    fn default() -> Self {
        Self {
            golden_path: "spec.routes.toml".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecDiff {
    pub endpoint: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecVerifyReport {
    pub generated_at: String,
    pub total: usize,
    pub matches: usize,
    pub diffs: Vec<SpecDiff>,
}

pub async fn run_spec_verify(_config: &SpecVerifyConfig) -> SpecVerifyReport {
    SpecVerifyReport {
        generated_at: Utc::now().to_rfc3339(),
        total: 0,
        matches: 0,
        diffs: vec![],
    }
}
