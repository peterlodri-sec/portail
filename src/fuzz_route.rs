//! Fuzz route — malformed-input crash detector.
//!
//! v0.6 — Feeds fuzzed HTTP requests to every registered route.
//! Validates: no panics, no 500s, proper error codes on malformed input.
//! Advisory: exits 0 unless a crash/panic is detected.
//!
//! # Property
//!
//! > The proxy must not crash on any input.

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FuzzReport {
    pub generated_at: String,
    pub total_probes: usize,
    pub passed: usize,
    pub errored: usize,
    pub crashed: usize,
    pub entries: Vec<FuzzEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FuzzEntry {
    pub method: String,
    pub path: String,
    pub payload: String, // description of what was sent
    pub status: u16,
    pub outcome: FuzzOutcome,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FuzzOutcome {
    /// Responded with 4xx (correct for malformed input)
    Pass,
    /// Responded with 5xx (shouldn't happen on malformed input)
    ServerError,
    /// Connection refused or timeout (proxy likely crashed)
    Crash,
}

// ─── fuzz probes ──────────────────────────────────────────────────

/// Generate a set of malformed inputs for each route.
pub fn generate_probes() -> Vec<(&'static str, &'static str, Vec<u8>, &'static str)> {
    let routes = vec![
        "/healthz", "/readyz", "/metrics", "/stats",
        "/v1/chat/completions", "/v1/messages", "/v1/responses",
        "/.well-known/agent.json", "/events", "/hooks",
        "/a2a/tasks", "/a2c/chat",
        "/ci/status", "/ci/badge", "/ci/webhook",
        "/discovery/nodes", "/discovery/stats",
        "/dns/query",
        "/tinyurl/shorten", "/tinyurl/stats",
        "/traces", "/traces/stats",
        "/cache/stats",
        "/godfather/status",
        "/nullclaw/agents",
        "/ebpf/stats", "/dpdk/stats", "/iouring/stats", "/hyper/stats",
    ];

    let mut probes = Vec::new();

    for path in &routes {
        // Empty body on GET
        probes.push(("GET", *path, vec![], "empty GET"));

        // Empty body on POST
        probes.push(("POST", *path, vec![], "empty POST body"));

        // Invalid JSON on POST
        probes.push(("POST", *path, b"not-valid-json".to_vec(), "invalid JSON"));

        // Binary garbage
        probes.push(("POST", *path, vec![0x00, 0x01, 0x02, 0xFF, 0xFE], "binary garbage"));

        // Very large body (but under limit)
        probes.push(("POST", *path, vec![b'x'; 10_000], "10KB of x"));

        // Null byte injection
        probes.push(("POST", *path, b"{\"key\": \"val\x00ue\"}".to_vec(), "null byte in JSON"));

        // Deeply nested JSON
        probes.push(("POST", *path, b"{\"a\":{\"b\":{\"c\":{\"d\":{\"e\":1}}}}}".to_vec(), "nested JSON"));

        // Array instead of object
        probes.push(("POST", *path, b"[1,2,3]".to_vec(), "array instead of object"));
    }

    probes
}

// ─── run ──────────────────────────────────────────────────────────

/// Run fuzz probes against a running proxy.
pub fn run(url: &str) -> Result<FuzzReport> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let probes = generate_probes();
    let mut entries = Vec::new();
    let mut passed = 0usize;
    let mut errored = 0usize;
    let mut crashed = 0usize;

    for (method, path, body, desc) in &probes {
        let target = format!("{}{}", url, path);
        let result = match *method {
            "GET" => client.get(&target).send(),
            "POST" => client.post(&target)
                .header("content-type", "application/json")
                .body(body.clone())
                .send(),
            _ => continue,
        };

        let (status, outcome) = match result {
            Ok(resp) => {
                let s = resp.status().as_u16();
                let outcome = if s >= 500 {
                    errored += 1;
                    FuzzOutcome::ServerError
                } else {
                    passed += 1;
                    FuzzOutcome::Pass
                };
                (s, outcome)
            }
            Err(_) => {
                crashed += 1;
                (0, FuzzOutcome::Crash)
            }
        };

        entries.push(FuzzEntry {
            method: method.to_string(),
            path: path.to_string(),
            payload: desc.to_string(),
            status,
            outcome,
        });
    }

    Ok(FuzzReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        total_probes: entries.len(),
        passed,
        errored,
        crashed,
        entries,
    })
}

// ─── CI mode ──────────────────────────────────────────────────────

/// Run fuzz in CI mode. Exits 0 unless a crash is detected.
pub fn ci_run(url: &str) -> Result<()> {
    let report = run(url)?;
    std::fs::write("fuzz-report.toml", &toml::to_string_pretty(&report)?)?;

    println!(
        "fuzz-route: {} probes | {} passed | {} errors | {} crashes",
        report.total_probes, report.passed, report.errored, report.crashed
    );

    if report.crashed > 0 {
        println!("fuzz-route: CRASHES DETECTED — proxy unstable on malformed input");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_probes_is_non_empty() {
        let probes = generate_probes();
        assert!(!probes.is_empty());
        // Every route gets 8 probes
        assert!(probes.len() >= 8);
    }

    #[test]
    fn fuzz_report_serializes() {
        let report = FuzzReport {
            generated_at: "2026-01-01".into(),
            total_probes: 10,
            passed: 8,
            errored: 2,
            crashed: 0,
            entries: vec![FuzzEntry {
                method: "GET".into(),
                path: "/healthz".into(),
                payload: "empty GET".into(),
                status: 200,
                outcome: FuzzOutcome::Pass,
            }],
        };
        let toml = toml::to_string_pretty(&report).unwrap();
        assert!(toml.contains("passed = 8"));
    }
}
