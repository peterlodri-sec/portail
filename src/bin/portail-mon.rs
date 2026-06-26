// Portail Monitor — animated ASCII stream dashboard.
// Connects to a running Portail instance and shows live stats.
//
// Usage:
//   cargo run --bin portail-mon [--port 8787] [--interval 2]
//
// Controls:
//   q / Ctrl+C  — quit
//   r           — refresh now

use std::io::{self, Write};
use std::time::Duration;

const LOGO: &[&str] = &[
    "  ██████  ██████  ██████  ████████  █████  ██ ██      ██",
    "  ██   ██ ██   ██ ██   ██    ██    ██   ██ ██ ██      ██",
    "  ██████  ██████  ██████     ██    ███████ ██ ██      ██",
    "  ██      ██   ██ ██   ██    ██    ██   ██ ██ ██      ██",
    "  ██      ██   ██ ██   ██    ██    ██   ██ ██ ██████  ██",
];

const FRAMES: &[&str] = &["▁ ▂ ▃ ▄ ▅ ▆ ▇ █", "█ ▇ ▆ ▅ ▄ ▃ ▂ ▁"];

fn clear_screen() {
    print!("\x1B[2J\x1B[H");
    io::stdout().flush().ok();
}

fn set_cursor(row: u16, col: u16) {
    print!("\x1B[{};{}H", row, col);
}

fn color(code: u8) {
    print!("\x1B[{}m", code);
}

fn render_logo(_frame: usize, highlight: usize) {
    let palette = [36, 33, 35, 32, 31]; // cyan, blue, magenta, green, red
    for (i, line) in LOGO.iter().enumerate() {
        let c = if i == highlight % 5 { 93 } else { palette[i] }; // bright yellow or palette
        color(c);
        println!("{}", line);
        color(0);
    }
}

fn render_stats(data: &str, frame: usize) {
    let bar = FRAMES[frame % FRAMES.len()];
    color(90);
    println!("\n  {}  live stream", bar);
    color(0);

    // Parse JSON stats and render
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
        if let Some(cdn) = val.get("cdn") {
            color(36);
            print!("  CDN  ");
            color(0);
            if let Some(hits) = cdn.get("hits").and_then(|v| v.as_u64()) {
                print!(" hits:{}", hits);
            }
            if let Some(misses) = cdn.get("misses").and_then(|v| v.as_u64()) {
                print!(" misses:{}", misses);
            }
            if let Some(purges) = cdn.get("purges").and_then(|v| v.as_u64()) {
                print!(" purges:{}", purges);
            }
            if let Some(entries) = cdn.get("memory_entries").and_then(|v| v.as_u64()) {
                print!(" mem_entries:{}", entries);
            }
            println!();
        }
        if let Some(version) = val.get("version").and_then(|v| v.as_str()) {
            color(90);
            println!("  version  {}", version);
            color(0);
        }
    } else {
        color(91);
        println!("  {}  (unparsable response)", data.trim());
        color(0);
    }
}

fn render_help() {
    color(90);
    println!("\n  [q] quit  [r] refresh");
    color(0);
}

fn render_connection_error(err: &str, attempt: u64) {
    color(91);
    println!("\n  ✗ connection failed (attempt {})", attempt);
    color(90);
    println!("  {}", err);
    color(0);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("PORTAIL_MON_PORT").unwrap_or_else(|_| "8787".into());
    let interval_secs: u64 = std::env::var("PORTAIL_MON_INTERVAL")
        .unwrap_or_else(|_| "2".into())
        .parse()
        .unwrap_or(2);
    let base_url = format!("http://127.0.0.1:{}", port);

    // Non-blocking input setup
    use tokio::io::AsyncBufReadExt;
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut input_line = String::new();

    let mut frame = 0usize;
    let mut attempt = 1u64;
    let mut errored = false;

    clear_screen();

    loop {
        // Input check (non-blocking)
        tokio::select! {
            result = stdin.read_line(&mut input_line) => {
                match result {
                    Ok(0) | Err(_) => break, // EOF
                    Ok(_) => {
                        let cmd = input_line.trim().to_lowercase();
                        input_line.clear();
                        match cmd.as_str() {
                            "q" | "quit" | "exit" => break,
                            _ => {} // 'r' fallthrough to refresh
                        }
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(interval_secs)) => {}
        }

        // Fetch stats
        let data = match reqwest::get(format!("{}/stats", base_url)).await {
            Ok(resp) => resp.text().await.unwrap_or_default(),
            Err(e) => {
                if !errored {
                    errored = true;
                    render_connection_error(&e.to_string(), attempt);
                    attempt += 1;
                }
                continue;
            }
        };
        errored = false;

        // Render
        set_cursor(1, 1);
        render_logo(frame, frame);
        render_stats(&data, frame);
        render_help();

        io::stdout().flush().ok();
        frame = frame.wrapping_add(1);
    }

    color(0);
    clear_screen();
    println!("portail-mon stopped");
    Ok(())
}
