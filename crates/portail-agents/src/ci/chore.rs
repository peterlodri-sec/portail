//! Chore Bot — mechanical cleanup automation. Advisory only.

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoreConfig {
    pub src_dir: String,
}

impl Default for ChoreConfig {
    fn default() -> Self {
        Self { src_dir: "src".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoreFinding {
    pub file: String,
    pub line: usize,
    pub issue: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoreReport {
    pub generated_at: String,
    pub total: usize,
    pub findings: Vec<ChoreFinding>,
}

pub fn run_chore_check(config: &ChoreConfig) -> ChoreReport {
    let mut findings = Vec::new();
    let src = std::path::Path::new(&config.src_dir);
    if src.exists() {
        for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") { continue; }
            if let Ok(content) = std::fs::read_to_string(path) {
                for (i, line) in content.lines().enumerate() {
                    if line.ends_with(' ') || line.ends_with('\t') {
                        findings.push(ChoreFinding {
                            file: path.to_string_lossy().to_string(),
                            line: i + 1,
                            issue: "trailing whitespace".into(),
                            severity: "low".into(),
                        });
                    }
                }
            }
        }
    }

    ChoreReport {
        generated_at: Utc::now().to_rfc3339(),
        total: findings.len(),
        findings,
    }
}
