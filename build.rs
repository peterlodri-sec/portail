//! Build script — embeds git hash + timestamp into the binary at compile time.
//! Always present, zero runtime cost, accessible via CLI and /dashboard.

fn main() {
    // Git commit hash (short)
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into())
        .trim()
        .to_string();

    // Git branch
    let git_branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into())
        .trim()
        .to_string();

    // Timestamp in ISO 8601
    let build_ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    println!("cargo:rustc-env=PORTAIL_GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=PORTAIL_GIT_BRANCH={}", git_branch);
    println!("cargo:rustc-env=PORTAIL_BUILD_TS={}", build_ts);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/main");
}
