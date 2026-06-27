//! Spec verify — route table introspection + golden-file enforcement.
//!
//! v0.5 — Generates a route table from the compiled axum Router, compares
//! against a golden file (`spec.routes.toml`). Posts diff as PR comment.
//! Advisory only — never fails CI.
//!
//! # Workflow
//!
//! 1. `portail spec-verify generate` — writes `spec.routes.toml`
//! 2. `portail spec-verify check` — compares against golden, reports diff
//! 3. `portail spec-verify --ci` — check + write report, always exit 0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

const GOLDEN_FILE: &str = "spec.routes.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteSpec {
    pub generated_at: String,
    pub version: String,
    pub routes: Vec<RouteEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteEntry {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecReport {
    pub generated_at: String,
    pub total_routes: usize,
    pub added: Vec<RouteEntry>,
    pub removed: Vec<RouteEntry>,
    pub changed: Vec<RouteChange>,
    pub has_diff: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteChange {
    pub path: String,
    pub old_methods: Vec<String>,
    pub new_methods: Vec<String>,
}

// ─── route table capture ──────────────────────────────────────────

/// Build the route spec from the running router.
/// Since axum Router doesn't expose route introspection at runtime,
/// we maintain the canonical route list here.
pub fn generate() -> RouteSpec {
    let routes = vec![
        // Health & metrics
        route("GET", "/healthz"),
        route("GET", "/livez"),
        route("GET", "/readyz"),
        route("GET", "/metrics"),
        // AI gateway
        route("ANY", "/v1/messages"),
        route("ANY", "/v1/chat/completions"),
        route("ANY", "/v1/responses"),
        route("ANY", "/v1/embeddings"),
        route("ANY", "/v1/audio/*"),
        route("ANY", "/v1/images/*"),
        route("ANY", "/v1beta/*"),
        // CDN
        route("ANY", "/cdn/*"),
        // MCP
        route("ANY", "/mcp/*"),
        route("ANY", "/mcp-rest/*"),
        // Stats
        route("GET", "/stats"),
        // Events
        route("GET", "/events"),
        route("POST", "/events"),
        route("GET", "/events/stream"),
        // Hooks
        route("GET", "/hooks"),
        route("POST", "/hooks"),
        route("DELETE", "/hooks/{id}"),
        // A2A (JSON-RPC 2.0)
        route("GET", "/.well-known/agent.json"),
        route("POST", "/a2a"),
        route("POST", "/a2a/subscribe"),
        // A2C
        route("POST", "/a2c/chat"),
        // CI
        route("GET", "/ci/status"),
        route("GET", "/ci/badge"),
        route("GET", "/ci/live"),
        route("POST", "/ci/webhook"),
        // Discovery
        route("POST", "/discovery/register"),
        route("POST", "/discovery/heartbeat/{id}"),
        route("POST", "/discovery/deregister/{id}"),
        route("GET", "/discovery/nodes"),
        route("GET", "/discovery/stats"),
        // DNS
        route("GET", "/dns/query"),
        // Plugins
        route("POST", "/tinyurl/shorten"),
        route("GET", "/tinyurl/stats"),
        route("GET", "/s/{id}"),
        route("GET", "/traces"),
        route("GET", "/traces/stats"),
        route("GET", "/traces/{id}"),
        route("GET", "/traces/{id}/ascii"),
        route("GET", "/cache/stats"),
        route("GET", "/cache/{key}"),
        route("POST", "/cache"),
        route("POST", "/cache/flush"),
        // Diagnostics
        route("GET", "/godfather/status"),
        route("GET", "/agents/status"),
    ];

    RouteSpec {
        generated_at: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        routes,
    }
}

fn route(method: &str, path: &str) -> RouteEntry {
    RouteEntry {
        method: method.to_string(),
        path: path.to_string(),
    }
}

// ─── check / diff ─────────────────────────────────────────────────

/// Compare generated spec against the golden file.
pub fn check() -> Result<SpecReport> {
    let current = generate();

    let golden_path = Path::new(GOLDEN_FILE);
    let golden: RouteSpec = if golden_path.exists() {
        toml::from_str(&std::fs::read_to_string(golden_path)?)?
    } else {
        return Ok(SpecReport {
            generated_at: current.generated_at.clone(),
            total_routes: current.routes.len(),
            added: current.routes.clone(),
            removed: vec![],
            changed: vec![],
            has_diff: !current.routes.is_empty(),
        });
    };

    compute_diff(&golden, &current)
}

fn compute_diff(golden: &RouteSpec, current: &RouteSpec) -> Result<SpecReport> {
    use std::collections::BTreeMap;

    let golden_map: BTreeMap<&str, Vec<&str>> = {
        let mut m: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for r in &golden.routes {
            m.entry(r.path.as_str())
                .or_default()
                .push(r.method.as_str());
        }
        m
    };

    let current_map: BTreeMap<&str, Vec<&str>> = {
        let mut m: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for r in &current.routes {
            m.entry(r.path.as_str())
                .or_default()
                .push(r.method.as_str());
        }
        m
    };

    let mut added = vec![];
    let mut removed = vec![];
    let mut changed = vec![];

    for (path, methods) in &current_map {
        if !golden_map.contains_key(path) {
            for m in methods {
                added.push(RouteEntry {
                    method: m.to_string(),
                    path: path.to_string(),
                });
            }
        } else {
            let g_methods = &golden_map[path];
            let cur_set: std::collections::BTreeSet<_> = methods.iter().collect();
            let g_set: std::collections::BTreeSet<_> = g_methods.iter().collect();
            if cur_set != g_set {
                changed.push(RouteChange {
                    path: path.to_string(),
                    old_methods: g_methods.iter().map(|s| s.to_string()).collect(),
                    new_methods: methods.iter().map(|s| s.to_string()).collect(),
                });
            }
        }
    }

    for path in golden_map.keys() {
        if !current_map.contains_key(path) {
            if let Some(methods) = golden_map.get(path) {
                for m in methods {
                    removed.push(RouteEntry {
                        method: m.to_string(),
                        path: path.to_string(),
                    });
                }
            }
        }
    }

    let has_diff = !added.is_empty() || !removed.is_empty() || !changed.is_empty();

    Ok(SpecReport {
        generated_at: current.generated_at.clone(),
        total_routes: current.routes.len(),
        added,
        removed,
        changed,
        has_diff,
    })
}

// ─── CLI ──────────────────────────────────────────────────────────

#[derive(clap::Subcommand, Debug, Clone)]
pub enum SpecCommand {
    /// Generate the golden spec file
    Generate,
    /// Check against the golden spec
    Check,
}

pub fn run(command: &SpecCommand, ci: bool) -> Result<()> {
    match command {
        SpecCommand::Generate => {
            let spec = generate();
            let toml = toml::to_string_pretty(&spec)?;
            std::fs::write(GOLDEN_FILE, &toml)?;
            println!(
                "spec-verify: wrote {} routes to {}",
                spec.routes.len(),
                GOLDEN_FILE
            );
            Ok(())
        }
        SpecCommand::Check => {
            let report = check()?;
            if ci {
                std::fs::write("spec-report.toml", &toml::to_string_pretty(&report)?)?;
            }
            if report.has_diff {
                println!("spec-verify: DRIFT DETECTED");
                for r in &report.added {
                    println!("  + {} {}", r.method, r.path);
                }
                for r in &report.removed {
                    println!("  - {} {}", r.method, r.path);
                }
                for c in &report.changed {
                    println!(
                        "  ~ {}  methods: {:?} -> {:?}",
                        c.path, c.old_methods, c.new_methods
                    );
                }
            } else {
                println!(
                    "spec-verify: {} routes match golden spec",
                    report.total_routes
                );
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_valid_spec() {
        let spec = generate();
        assert!(!spec.routes.is_empty());
        assert!(spec.routes.iter().any(|r| r.path == "/healthz"));
    }

    #[test]
    fn identical_specs_have_no_diff() {
        let spec = generate();
        let report = compute_diff(&spec, &spec).unwrap();
        assert!(!report.has_diff);
        assert_eq!(report.added.len(), 0);
        assert_eq!(report.removed.len(), 0);
    }

    #[test]
    fn added_route_is_detected() {
        let golden = generate();
        let mut current = generate();
        current.routes.push(RouteEntry {
            method: "GET".into(),
            path: "/new-route".into(),
        });
        let report = compute_diff(&golden, &current).unwrap();
        assert!(report.has_diff);
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].path, "/new-route");
    }

    #[test]
    fn removed_route_is_detected() {
        let mut golden = generate();
        let current = generate();
        golden.routes.push(RouteEntry {
            method: "GET".into(),
            path: "/old-route".into(),
        });
        let report = compute_diff(&golden, &current).unwrap();
        assert!(report.has_diff);
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].path, "/old-route");
    }
}
