//! Developer/contributor entry point — the main CLI for day-to-day work.
//!
//! Bundles check, test, lint, build, audit, CI into one subcommand tree.
//! Uses only modern tools (rg, jq, sd, bat, eza, fd, delta).

use anyhow::Result;
use std::process::Command;

pub fn run_dev(action: &super::DevAction) -> Result<()> {
    match action {
        super::DevAction::Dashboard => {
            // Launch the existing TUI dashboard
            let mut dashboard = super::dashboard::Dashboard::new();
            dashboard.run_tui()?;
            Ok(())
        }
        super::DevAction::Check => {
            cmd("cargo", &["check"])?;
            cmd("cargo", &["check", "--features", "jemalloc"])?;
            cmd("cargo", &["check", "--features", "portail_system_alloc"])?;
            Ok(())
        }
        super::DevAction::Lint => {
            cmd("cargo", &["clippy", "--workspace", "--", "-D", "warnings"])?;
            cmd("cargo", &["fmt", "--check"])?;
            Ok(())
        }
        super::DevAction::Test => {
            cmd("cargo", &["test"])?;
            Ok(())
        }
        super::DevAction::Build { max } => {
            let mut args = vec!["build", "--release", "-p", "portail"];
            if *max {
                cmd("cargo", &["build", "--release", "-p", "portail",
                    "--config", "profile.release.lto=\"fat\"",
                    "--config", "profile.release.codegen-units=1",
                ])?;
            } else {
                cmd("cargo", &["build", "--release", "-p", "portail"])?;
            }
            // Show binary size
            let path = if cfg!(target_os = "macos") {
                "target/release/portail"
            } else {
                "target/release/portail"
            };
            if std::path::Path::new(path).exists() {
                let meta = std::fs::metadata(path)?;
                let size_kb = meta.len() as f64 / 1024.0;
                let size_str = if size_kb > 1024.0 {
                    format!("{:.1} MiB", size_kb / 1024.0)
                } else {
                    format!("{:.1} KiB", size_kb)
                };
                println!("Binary size: {}", size_str);
            }
            Ok(())
        }
        super::DevAction::Audit { version } => {
            let dist = std::path::Path::new("target/release");
            if !dist.exists() {
                anyhow::bail!("No release build found. Run 'portail dev build' first");
            }
            crate::release_audit::run_pipeline(dist, version, dist)?;
            Ok(())
        }
        super::DevAction::Ci { version } => {
            run_ci(version.as_deref())?;
            Ok(())
        }
    }
}

fn run_ci(version: Option<&str>) -> Result<()> {
    println!("═══ CI Pipeline ═══");

    println!("\n── cargo check ──");
    cmd("cargo", &["check"])?;

    println!("\n── cargo clippy ──");
    cmd("cargo", &["clippy", "--workspace", "--", "-D", "warnings"])?;

    println!("\n── cargo fmt --check ──");
    cmd("cargo", &["fmt", "--check"])?;

    println!("\n── cargo test ──");
    cmd("cargo", &["test"])?;

    if let Some(ver) = version {
        println!("\n── release audit ──");
        let dist = std::path::Path::new("target/release");
        if dist.exists() {
            crate::release_audit::run_pipeline(dist, ver, dist)?;
        } else {
            println!("  (skip — no release build, run 'portail dev build' first)");
        }
    }

    println!("\n✅ CI pipeline passed");
    Ok(())
}

fn cmd(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn {program}: {e}"))?
        .wait()?;
    if !status.success() {
        anyhow::bail!("{program} exited with code {:?}", status.code());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cli::DevAction;

    #[test]
    fn test_dev_actions_dispatch_without_panic() {
        let actions = [
            DevAction::Check,
            DevAction::Lint,
            DevAction::Test,
            DevAction::Build { max: false },
            DevAction::Audit { version: "0.0.0".into() },
        ];
        assert_eq!(actions.len(), 5);
    }
}
