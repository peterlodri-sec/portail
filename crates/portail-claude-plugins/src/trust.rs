//! Trust scoring — 1-10 scale, metrics-driven.
//!
//! Each plugin gets a trust score based on:
//! - Error rate (hooks that fail)
//! - Latency (hook execution time)
//! - Resource usage (memory, CPU)
//! - Manual overrides
//!
//! Trust affects which hooks execute and which tools are available.
//! A plugin with trust < 3 gets sandboxed (WASM only).
//! A plugin with trust >= 8 gets elevated permissions.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

/// Trust score on a 1-10 scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    /// Current trust score (1-10).
    pub score: u8,
    /// Total hook executions.
    pub total_executions: u64,
    /// Failed hook executions.
    pub failed_executions: u64,
    /// Recent execution latencies (last 100).
    pub recent_latencies: VecDeque<Duration>,
    /// Manual override (if set, overrides computed score).
    pub override_score: Option<u8>,
    /// Timestamp of last score computation.
    pub last_computed: Option<u64>,
}

impl Default for TrustScore {
    fn default() -> Self {
        Self {
            score: 5, // neutral starting trust
            total_executions: 0,
            failed_executions: 0,
            recent_latencies: VecDeque::with_capacity(100),
            override_score: None,
            last_computed: None,
        }
    }
}

impl TrustScore {
    /// Create a new trust score with default value 5.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a specific initial score.
    pub fn with_score(score: u8) -> Self {
        Self {
            score: score.clamp(1, 10),
            ..Default::default()
        }
    }

    /// Record a successful hook execution.
    pub fn record_success(&mut self, latency: Duration) {
        self.total_executions += 1;
        if self.recent_latencies.len() >= 100 {
            self.recent_latencies.pop_front();
        }
        self.recent_latencies.push_back(latency);
        self.recompute();
    }

    /// Record a failed hook execution.
    pub fn record_failure(&mut self, latency: Duration) {
        self.total_executions += 1;
        self.failed_executions += 1;
        if self.recent_latencies.len() >= 100 {
            self.recent_latencies.pop_front();
        }
        self.recent_latencies.push_back(latency);
        self.recompute();
    }

    /// Set a manual override score.
    pub fn set_override(&mut self, score: u8) {
        self.override_score = Some(score.clamp(1, 10));
        self.score = self.override_score.unwrap();
    }

    /// Clear manual override and recompute.
    pub fn clear_override(&mut self) {
        self.override_score = None;
        self.recompute();
    }

    /// Get the effective score (override or computed).
    pub fn effective_score(&self) -> u8 {
        self.override_score.unwrap_or(self.score)
    }

    /// Whether the plugin is trusted (score >= 7).
    pub fn is_trusted(&self) -> bool {
        self.effective_score() >= 7
    }

    /// Whether the plugin should be sandboxed (score < 3).
    pub fn should_sandbox(&self) -> bool {
        self.effective_score() < 3
    }

    /// Permission level based on trust.
    pub fn permission_level(&self) -> PermissionLevel {
        match self.effective_score() {
            1..=2 => PermissionLevel::Sandboxed,
            3..=4 => PermissionLevel::Restricted,
            5..=6 => PermissionLevel::Standard,
            7..=8 => PermissionLevel::Elevated,
            9..=10 => PermissionLevel::Full,
            _ => PermissionLevel::Standard,
        }
    }

    /// Recompute the trust score from metrics.
    fn recompute(&mut self) {
        if self.override_score.is_some() {
            return;
        }

        if self.total_executions == 0 {
            self.score = 5;
            return;
        }

        // Factor 1: Error rate (0-4 points)
        let error_rate = self.failed_executions as f64 / self.total_executions as f64;
        let error_score = if error_rate == 0.0 {
            4.0
        } else if error_rate < 0.01 {
            3.0
        } else if error_rate < 0.05 {
            2.0
        } else if error_rate < 0.10 {
            1.0
        } else {
            0.0
        };

        // Factor 2: Latency (0-3 points)
        let latency_score = if self.recent_latencies.is_empty() {
            1.5 // neutral
        } else {
            let avg_ms: f64 = self
                .recent_latencies
                .iter()
                .map(|d| d.as_millis() as f64)
                .sum::<f64>()
                / self.recent_latencies.len() as f64;
            if avg_ms < 10.0 {
                3.0
            } else if avg_ms < 50.0 {
                2.5
            } else if avg_ms < 200.0 {
                2.0
            } else if avg_ms < 1000.0 {
                1.0
            } else {
                0.0
            }
        };

        // Factor 3: Volume bonus (0-3 points) — more successful runs = more trust
        let volume_score = if self.total_executions > 1000 {
            3.0
        } else if self.total_executions > 100 {
            2.0
        } else if self.total_executions > 10 {
            1.0
        } else {
            0.0
        };

        let raw: f64 = error_score + latency_score + volume_score;
        // Map 0-10 to 1-10
        self.score = (raw.round() as u8).clamp(1, 10);
        self.last_computed = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Score 1-2: WASM only, no file/network access.
    Sandboxed,
    /// Score 3-4: Limited tools, no write access.
    Restricted,
    /// Score 5-6: Standard tool access.
    Standard,
    /// Score 7-8: Elevated — can access sensitive tools.
    Elevated,
    /// Score 9-10: Full — unrestricted access.
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_trust_is_neutral() {
        let t = TrustScore::new();
        assert_eq!(t.score, 5);
        assert!(!t.is_trusted());
        assert!(!t.should_sandbox());
    }

    #[test]
    fn success_improves_trust() {
        let mut t = TrustScore::new();
        for _ in 0..100 {
            t.record_success(Duration::from_millis(5));
        }
        assert!(t.score >= 7);
        assert!(t.is_trusted());
    }

    #[test]
    fn failures_decrease_trust() {
        let mut t = TrustScore::new();
        for _ in 0..10 {
            t.record_failure(Duration::from_millis(5));
        }
        // 100% error rate but fast latency + low volume = score 3
        assert!(t.score <= 3);
    }

    #[test]
    fn override_takes_precedence() {
        let mut t = TrustScore::new();
        t.set_override(9);
        assert_eq!(t.effective_score(), 9);
        assert!(t.is_trusted());

        t.clear_override();
        assert_eq!(t.score, 5); // back to default
    }

    #[test]
    fn permission_levels() {
        let mut t = TrustScore::new();
        t.set_override(1);
        assert_eq!(t.permission_level(), PermissionLevel::Sandboxed);

        t.set_override(4);
        assert_eq!(t.permission_level(), PermissionLevel::Restricted);

        t.set_override(6);
        assert_eq!(t.permission_level(), PermissionLevel::Standard);

        t.set_override(8);
        assert_eq!(t.permission_level(), PermissionLevel::Elevated);

        t.set_override(10);
        assert_eq!(t.permission_level(), PermissionLevel::Full);
    }

    #[test]
    fn slow_latency_reduces_trust() {
        let mut t = TrustScore::new();
        for _ in 0..100 {
            t.record_success(Duration::from_secs(5)); // very slow
        }
        // Even with all successes, slow latency caps trust
        assert!(t.score < 7);
    }

    #[test]
    fn mixed_results() {
        let mut t = TrustScore::new();
        // 90% success, 10% failure
        for _ in 0..90 {
            t.record_success(Duration::from_millis(10));
        }
        for _ in 0..10 {
            t.record_failure(Duration::from_millis(10));
        }
        // Should be moderate trust
        assert!(t.score >= 3 && t.score <= 7);
    }
}
