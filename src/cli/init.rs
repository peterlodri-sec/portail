use anyhow::Result;
use std::io::{self, Write};

pub fn run_init() -> Result<()> {
    let mut out = String::new();

    println!("Portail init wizard — generate portail.toml\n");
    println!("Press Enter to accept defaults (shown in [brackets]).\n");

    // ── Listen address ──
    let listen = ask("Listen address", "0.0.0.0:8787");
    out.push_str(&format!("listen = \"{}\"\n", listen));

    // ── Rate limiting ──
    out.push('\n');
    let rate_enabled = ask_bool("Enable rate limiting", true);
    out.push_str(&format!("[rate_limit]\nenabled = {}\n", rate_enabled));
    if rate_enabled {
        let burst = ask("Burst size", "30");
        let per_second = ask("Tokens per second", "10.0");
        out.push_str(&format!("burst = {}\nper_second = {}\n", burst, per_second));
    }

    // ── Authentication ──
    out.push('\n');
    let auth_enabled = ask_bool("Enable authentication", false);
    out.push_str(&format!("[auth]\nenabled = {}\n", auth_enabled));
    if auth_enabled {
        println!("  (add API keys manually in [auth.api_keys] section)");
    }

    // ── AI Gateway ──
    out.push('\n');
    let gw_enabled = ask_bool("Enable AI Gateway proxy", false);
    out.push_str(&format!("[ai_gateway]\nenabled = {}\n", gw_enabled));
    if gw_enabled {
        let upstream = ask("Upstream URL", "http://127.0.0.1:4000");
        out.push_str(&format!("upstream = \"{}\"\n", upstream));
    }

    // ── MCP sidecar ──
    out.push('\n');
    let mcp_enabled = ask_bool("Enable MCP sidecar", false);
    out.push_str(&format!("[mcp]\nenabled = {}\n", mcp_enabled));
    if mcp_enabled {
        let socket = ask("Socket path", "/run/portail/mcp.sock");
        out.push_str(&format!("socket_path = \"{}\"\n", socket));
    }

    // ── CDN cache ──
    out.push('\n');
    let cdn_enabled = ask_bool("Enable CDN cache", false);
    out.push_str(&format!("[cdn]\nenabled = {}\n", cdn_enabled));
    if cdn_enabled {
        let origin = ask("Origin URL", "http://127.0.0.1:9000");
        let cache_dir = ask("Cache directory", "/var/cache/portail");
        let cache_size = ask("Max cache size", "10g");
        out.push_str(&format!(
            "origin = \"{}\"\ncache_dir = \"{}\"\ncache_size = \"{}\"\n",
            origin, cache_dir, cache_size
        ));
    }

    // ── Telemetry (OTLP) ──
    out.push('\n');
    let otlp_enabled = ask_bool("Enable OTLP trace export", false);
    out.push_str(&format!("[telemetry]\nenabled = {}\n", otlp_enabled));
    if otlp_enabled {
        let endpoint = ask("OTLP endpoint", "http://localhost:4317");
        out.push_str(&format!("endpoint = \"{}\"\n", endpoint));
    }

    // ── Event store ──
    out.push('\n');
    let store_enabled = ask_bool("Enable persistent event store (SQLite)", true);
    out.push_str(&format!("[store]\nenabled = {}\n", store_enabled));
    if store_enabled {
        let db_path = ask("Database path", "portail-events.db");
        let retention = ask("Retention period (e.g., 30d)", "30d");
        out.push_str(&format!(
            "db_path = \"{}\"\nretention = \"{}\"\n",
            db_path, retention
        ));
    }

    // ── Write file ──
    let path = "portail.toml";
    if std::path::Path::new(path).exists() {
        let overwrite = ask_bool("portail.toml already exists. Overwrite?", false);
        if !overwrite {
            println!("\nCancelled. Existing portail.toml was not modified.");
            return Ok(());
        }
    }

    std::fs::write(path, &out)?;
    println!("\nportail.toml generated successfully.");
    println!("Run 'portail serve' to start, or 'portail' for the TUI.\n");
    println!("Full config reference: https://github.com/peterlodri-sec/portail");

    Ok(())
}

fn ask(prompt: &str, default: &str) -> String {
    print!("  {} [{}]: ", prompt, default);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn ask_bool(prompt: &str, default: bool) -> bool {
    let default_str = if default { "Y/n" } else { "y/N" };
    print!("  {} [{}]: ", prompt, default_str);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return default;
    }
    matches!(trimmed.as_str(), "y" | "yes" | "true" | "1")
}
