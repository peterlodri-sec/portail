//! Build metadata — embedded at compile time via build.rs.
//!
//! Always available in the binary. Use these for version reporting,
//! CI status, and debugging deployed instances.

/// Git commit hash (short, e.g., "b2a2fa8").
pub fn git_hash() -> &'static str {
    option_env!("PORTAIL_GIT_HASH").unwrap_or("unknown")
}

/// Git branch name (e.g., "main").
pub fn git_branch() -> &'static str {
    option_env!("PORTAIL_GIT_BRANCH").unwrap_or("unknown")
}

/// Build timestamp in ISO 8601 (e.g., "2026-06-26T12:34:56Z").
pub fn build_ts() -> &'static str {
    option_env!("PORTAIL_BUILD_TS").unwrap_or("unknown")
}

/// Full version string for display.
pub fn version_string() -> String {
    format!(
        "{} ({} {} {})",
        env!("CARGO_PKG_VERSION"),
        git_hash(),
        git_branch(),
        build_ts(),
    )
}
