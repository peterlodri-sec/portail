use anyhow::Result;

/// System compatibility check — reports what's working and what needs fixing.
pub fn run_doctor() -> Result<()> {
    let mut issues = 0u32;

    println!("Portail Doctor — v{}\n", env!("CARGO_PKG_VERSION"));

    // ── Rust toolchain ──
    println!("═══ Toolchain ═══");
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output();
    match rustc {
        Ok(o) => {
            let v = String::from_utf8_lossy(&o.stdout);
            println!("  ✓ rustc: {}", v.trim());
        }
        Err(_) => {
            println!("  ✗ rustc not found. Install from https://rustup.rs");
            issues += 1;
        }
    }

    // ── OS + arch ──
    println!("\n═══ System ═══");
    println!("  OS:    {}", std::env::consts::OS);
    println!("  Arch:  {}", std::env::consts::ARCH);

    // ── Kernel features ──
    if cfg!(target_os = "linux") {
        // Check io_uring support
        let uname = std::process::Command::new("uname").arg("-r").output();
        if let Ok(o) = uname {
            let kernel = String::from_utf8_lossy(&o.stdout);
            // Parse major.minor
            let parts: Vec<u32> = kernel
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 2 && (parts[0] > 5 || (parts[0] == 5 && parts[1] >= 1)) {
                println!("  ✓ Kernel {} supports io_uring", kernel.trim());
            } else {
                println!("  ! Kernel {} — io_uring requires 5.1+", kernel.trim());
            }
        }
    }

    // ── Ports ──
    println!("\n═══ Network ═══");
    let port = 8787u16;
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => println!("  ✓ Port {} is free", port),
        Err(_) => {
            println!("  ✗ Port {} is already in use", port);
            issues += 1;
        }
    }

    // ── Config ──
    println!("\n═══ Config ═══");
    let config_path = std::path::Path::new("portail.toml");
    if config_path.exists() {
        match crate::config::Config::load(Some(config_path)) {
            Ok(cfg) => {
                println!("  ✓ portail.toml is valid");
                println!("    listen: {}", cfg.listen);
                println!(
                    "    rate_limit: {}",
                    if cfg.rate_limit.enabled { "on" } else { "off" }
                );
                println!("    auth: {}", if cfg.auth.enabled { "on" } else { "off" });
                println!(
                    "    store: {} (provider: {})",
                    if cfg.store.enabled { "on" } else { "off" },
                    cfg.store.provider
                );
            }
            Err(e) => {
                println!("  ✗ portail.toml is invalid: {}", e);
                issues += 1;
            }
        }
    } else {
        println!("  ! No portail.toml found. Run 'portail init' to create one.");
        println!("    Portail works without a config file — defaults will be used.");
    }

    // ── Dependencies ──
    println!("\n═══ Optional Dependencies ═══");
    let dep_checks = [
        ("Redis", "redis-cli", "redis-cli ping"),
        ("NATS", "nats", "nats --version"),
    ];
    for (name, bin, _) in &dep_checks {
        if which::which(bin).is_ok() {
            println!("  ✓ {} available", name);
        } else {
            println!("  ! {} not found (optional)", name);
        }
    }

    // ── Summary ──
    println!("\n═══ Summary ═══");
    if issues == 0 {
        println!("  ✓ All checks passed. Ready to serve!");
    } else {
        println!(
            "  ⚠ {} issue(s) found. Fix the items marked ✗ above.",
            issues
        );
    }
    println!("\n  Run 'portail serve' to start the server.");

    Ok(())
}
