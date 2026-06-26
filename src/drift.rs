//! Drift detect — production traffic replay for regression testing.
//!
//! v0.4 — Captures real request/response pairs, replays them in CI,
//! compares SHA-256 of responses. Posts diff report as PR comment.
//! Advisory only — never fails CI.
//!
//! # Workflow
//!
//! 1. **Capture**: `portail drift-detect capture --url http://localhost:8787`
//!    → records requests to `.drift/session-{timestamp}.json`
//! 2. **Replay**: `portail drift-detect replay`
//!    → sends captured requests to proxy, compares responses
//! 3. **CI mode**: `portail drift-detect --ci`
//!    → replays + writes `drift-report.toml`, always exits 0

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ─── data model ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftSession {
    pub captured_at: String,
    pub source_url: String,
    pub entries: Vec<DriftEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEntry {
    pub method: String,
    pub path: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: String,
    pub response_status: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: String,
    pub response_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub generated_at: String,
    pub total: usize,
    pub matched: usize,
    pub drifted: usize,
    pub errors: usize,
    pub entries: Vec<DriftDiff>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftStatus {
    #[serde(rename = "match")]
    Match,
    #[serde(rename = "drifted")]
    Drifted,
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriftDiff {
    pub method: String,
    pub path: String,
    pub status: DriftStatus,
    pub original_sha256: String,
    pub replay_sha256: Option<String>,
    pub detail: Option<String>,
}

// ─── defaults ─────────────────────────────────────────────────────

const DRIFT_DIR: &str = ".drift";

// ─── capture ──────────────────────────────────────────────────────

/// Capture requests by sending pre-defined probes to a running proxy.
pub fn capture(url: &str) -> Result<DriftSession> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let probes: Vec<DriftEntry> = vec![
        probe(&client, url, "GET", "/healthz", "")?,
        probe(&client, url, "GET", "/readyz", "")?,
        probe(&client, url, "GET", "/metrics", "")?,
        probe(&client, url, "GET", "/.well-known/agent.json", "")?,
        probe(&client, url, "GET", "/stats", "")?,
        probe_json(&client, url, "POST", "/v1/chat/completions", r#"{"model":"test","messages":[{"role":"user","content":"hi"}]}"#)?,
    ];

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Ok(DriftSession {
        captured_at: chrono::Utc::now().to_rfc3339(),
        source_url: url.to_string(),
        entries: probes,
    })
}

fn probe(
    client: &reqwest::blocking::Client,
    base: &str,
    method: &str,
    path: &str,
    _body: &str,
) -> Result<DriftEntry> {
    let url = format!("{}{}", base, path);
    let resp = match method {
        "GET" => client.get(&url).send()?,
        _ => client.get(&url).send()?,
    };

    let status = resp.status().as_u16();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let resp_body = resp.text()?;
    let sha = sha256_hex(&resp_body);

    Ok(DriftEntry {
        method: method.to_string(),
        path: path.to_string(),
        request_headers: vec![],
        request_body: String::new(),
        response_status: status,
        response_headers: resp_headers,
        response_body: resp_body,
        response_sha256: sha,
    })
}

fn probe_json(
    client: &reqwest::blocking::Client,
    base: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<DriftEntry> {
    let url = format!("{}{}", base, path);
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()?;

    let status = resp.status().as_u16();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let resp_body = resp.text()?;
    let sha = sha256_hex(&resp_body);

    Ok(DriftEntry {
        method: method.to_string(),
        path: path.to_string(),
        request_headers: vec![],
        request_body: body.to_string(),
        response_status: status,
        response_headers: resp_headers,
        response_body: resp_body,
        response_sha256: sha,
    })
}

// ─── replay ───────────────────────────────────────────────────────

/// Replay a session against a running proxy and produce a diff report.
pub fn replay(url: &str, session: &DriftSession) -> Result<DriftReport> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut diffs = Vec::new();
    let mut matched = 0;
    let mut drifted = 0;
    let mut errors = 0;

    for entry in &session.entries {
        let result = replay_entry(&client, url, entry);
        match result {
            Ok(diff) => {
                match diff.status {
                    DriftStatus::Match => matched += 1,
                    DriftStatus::Drifted => drifted += 1,
                    DriftStatus::Error => errors += 1,
                }
                diffs.push(diff);
            }
            Err(e) => {
                errors += 1;
                diffs.push(DriftDiff {
                    method: entry.method.clone(),
                    path: entry.path.clone(),
                    status: DriftStatus::Error,
                    original_sha256: entry.response_sha256.clone(),
                    replay_sha256: None,
                    detail: Some(format!("{}", e)),
                });
            }
        }
    }

    Ok(DriftReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        total: session.entries.len(),
        matched,
        drifted,
        errors,
        entries: diffs,
    })
}

fn replay_entry(
    client: &reqwest::blocking::Client,
    base: &str,
    entry: &DriftEntry,
) -> Result<DriftDiff> {
    let url = format!("{}{}", base, entry.path);
    let resp = match entry.method.as_str() {
        "GET" => client.get(&url).send()?,
        "POST" => client
            .post(&url)
            .header("content-type", "application/json")
            .body(entry.request_body.clone())
            .send()?,
        _ => client.get(&url).send()?,
    };

    let resp_body = resp.text()?;
    let replay_sha = sha256_hex(&resp_body);

    let status = if replay_sha == entry.response_sha256 {
        DriftStatus::Match
    } else {
        DriftStatus::Drifted
    };

    Ok(DriftDiff {
        method: entry.method.clone(),
        path: entry.path.clone(),
        status: status.into(),
        original_sha256: entry.response_sha256.clone(),
        replay_sha256: Some(replay_sha),
        detail: None,
    })
}

// ─── persistence ──────────────────────────────────────────────────

/// Save a session to the drift directory.
pub fn save_session(session: &DriftSession) -> Result<PathBuf> {
    let dir = Path::new(DRIFT_DIR);
    std::fs::create_dir_all(dir)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let path = dir.join(format!("session-{}.json", ts));
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(&path, json)?;
    eprintln!("drift-detect: saved session to {}", path.display());
    Ok(path)
}

/// Load the most recent session from the drift directory.
pub fn load_latest_session() -> Result<DriftSession> {
    let dir = Path::new(DRIFT_DIR);
    let mut sessions: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    sessions.sort_by_key(|e| e.path());
    let latest = sessions.last().ok_or_else(|| {
        anyhow::anyhow!("no drift sessions found in {}", DRIFT_DIR)
    })?;
    let json = std::fs::read_to_string(latest.path())?;
    Ok(serde_json::from_str(&json)?)
}

// ─── CI mode ──────────────────────────────────────────────────────

/// Run capture + replay in CI mode. Always exits 0.
pub fn ci_run(url: &str) -> Result<String> {
    let session = match capture(url) {
        Ok(s) => {
            save_session(&s)?;
            s
        }
        Err(e) => {
            let report = DriftReport {
                generated_at: chrono::Utc::now().to_rfc3339(),
                total: 0,
                matched: 0,
                drifted: 0,
                errors: 1,
                entries: vec![DriftDiff {
                    method: "N/A".into(),
                    path: "N/A".into(),
                    status: DriftStatus::Error,
                    original_sha256: String::new(),
                    replay_sha256: None,
                    detail: Some(format!("capture failed: {}", e)),
                }],
            };
            let toml = toml::to_string_pretty(&report).unwrap_or_default();
            std::fs::write("drift-report.toml", &toml)?;
            return Ok(toml);
        }
    };

    let report = replay(url, &session).unwrap_or_else(|e| DriftReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        total: 0,
        matched: 0,
        drifted: 0,
        errors: 1,
        entries: vec![DriftDiff {
            method: "N/A".into(),
            path: "N/A".into(),
            status: DriftStatus::Error,
            original_sha256: String::new(),
            replay_sha256: None,
            detail: Some(format!("replay failed: {}", e)),
        }],
    });

    let toml = toml::to_string_pretty(&report).unwrap_or_default();
    std::fs::write("drift-report.toml", &toml)?;

    // Print summary for CI log
    println!(
        "drift-detect: {} total | {} matched | {} drifted | {} errors",
        report.total, report.matched, report.drifted, report.errors
    );

    Ok(toml)
}

fn sha256_hex(data: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

// ─── CLI command ──────────────────────────────────────────────────

#[derive(clap::Subcommand, Debug, Clone)]
pub enum DriftCommand {
    /// Capture a drift session from a running proxy
    Capture {
        #[arg(long, default_value = "http://localhost:8787")]
        url: String,
    },
    /// Replay the latest captured session
    Replay {
        #[arg(long, default_value = "http://localhost:8787")]
        url: String,
    },
}

pub fn run(command: &DriftCommand, ci: bool) -> Result<()> {
    match command {
        DriftCommand::Capture { url } => {
            let session = capture(url)?;
            save_session(&session)?;
            if ci {
                std::fs::write("drift-session.json", &serde_json::to_string_pretty(&session)?)?;
            }
            println!("drift-detect: captured {} probes from {}", session.entries.len(), url);
            Ok(())
        }
        DriftCommand::Replay { url } => {
            let session = load_latest_session()?;
            let report = replay(url, &session)?;
            if ci {
                std::fs::write("drift-report.toml", &toml::to_string_pretty(&report)?)?;
            }
            println!(
                "drift-detect: {} total | {} matched | {} drifted | {} errors",
                report.total, report.matched, report.drifted, report.errors
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_stable() {
        let a = sha256_hex("hello");
        let b = sha256_hex("hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn drift_entry_roundtrip_json() {
        let entry = DriftEntry {
            method: "GET".into(),
            path: "/healthz".into(),
            request_headers: vec![],
            request_body: String::new(),
            response_status: 200,
            response_headers: vec![],
            response_body: "ok".into(),
            response_sha256: sha256_hex("ok"),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: DriftEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.response_sha256, sha256_hex("ok"));
    }

    #[test]
    fn drift_report_serializes() {
        let report = DriftReport {
            generated_at: "2026-01-01".into(),
            total: 1,
            matched: 1,
            drifted: 0,
            errors: 0,
            entries: vec![DriftDiff {
                method: "GET".into(),
                path: "/healthz".into(),
                status: DriftStatus::Match,
                original_sha256: "abc".into(),
                replay_sha256: Some("abc".into()),
                detail: None,
            }],
        };
        let toml = toml::to_string_pretty(&report).unwrap();
        assert!(toml.contains("match"));
    }
}
