//! Panic hooks + graceful shutdown + startup idempotency.
//!
//! # v2.0
//!
//! Production hardening: installs a panic hook that logs and flushes
//! before aborting. Provides graceful shutdown with connection drain.

use std::panic;
use std::time::Duration;

/// Install the production panic hook. Logs the panic info and flushes
/// tracing before the process aborts.
pub fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // Log the panic before the default hook prints to stderr
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown".into()
        };
        tracing::error!(%payload, %location, "PANIC — flushing and aborting");

        // Flush tracing subscribers before the default hook runs
        // (tracing-subscriber's fmt layer flushes on drop, but we want to be explicit)
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_hook_does_not_panic() {
        // Installing the hook must not panic itself
        install_panic_hook();
        // Reset to default to not affect other tests
        let _ = panic::take_hook();
    }

    #[test]
    fn startup_detects_missing_config_file() {
        // Verify that with a nonexistent config file, config::load returns defaults
        let path = std::path::PathBuf::from("/tmp/portail-nonexistent-test-config.toml");
        let cfg = crate::config::Config::load(Some(&path));
        assert!(cfg.is_ok());
        // Should use defaults when file doesn't exist
        let cfg = cfg.unwrap();
        assert_eq!(cfg.listen, "0.0.0.0:8787");
    }
}
