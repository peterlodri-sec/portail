//! Process Interception Tracker (PIT).
//!
//! Watches all new processes in the box, logs them to a PIT file,
//! and optionally intercepts via the execve hook (when LD_PRELOAD is active).
//!
//! PIT file: structured JSONL log of every process start.
//! Ignore list: known-safe processes (editor, shell, etc.) — not logged.
//! Integration: feeds into supervisor for restart/respawn decisions.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

// ── PIT Entry ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitEntry {
    pub timestamp: String,
    pub pid: u32,
    pub ppid: u32,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub intercepted: bool,
    pub handler: Option<String>,
}

// ── PIT Configuration ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitConfig {
    /// Path to the PIT log file (JSONL)
    pub pit_path: PathBuf,
    /// Path to the human-readable audit log
    pub log_path: PathBuf,
    /// Whether to actively intercept matching commands
    pub intercept_enabled: bool,
    /// Commands to NEVER intercept (pass through to real binary)
    pub ignore_list: HashSet<String>,
    /// Commands that we DO intercept (git, gh, make, etc.)
    pub intercept_list: HashSet<String>,
    /// Whether to watch /proc for new PIDs (Linux only)
    pub proc_watch: bool,
}

impl Default for PitConfig {
    fn default() -> Self {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/var/lib"))
            .join("portail");
        Self {
            pit_path: base.join("pit.jsonl"),
            log_path: base.join("pit.log"),
            intercept_enabled: true,
            ignore_list: HashSet::from_iter([
                "bash".into(),
                "zsh".into(),
                "fish".into(),
                "sh".into(),
                "nvim".into(),
                "vim".into(),
                "code".into(),
                "cursor".into(),
                "tmux".into(),
                "screen".into(),
                "sudo".into(),
                "su".into(),
            ]),
            intercept_list: HashSet::from_iter([
                "git".into(),
                "gh".into(),
                "make".into(),
                "curl".into(),
            ]),
            proc_watch: cfg!(target_os = "linux"),
        }
    }
}

// ── PIT Engine ────────────────────────────────────────────────────

pub struct Pit {
    config: PitConfig,
    pit_file: Mutex<fs::File>,
    log_file: Mutex<fs::File>,
    seen_pids: Mutex<HashSet<u32>>,
}

impl Pit {
    pub fn new(config: PitConfig) -> std::io::Result<Self> {
        let dir = config.pit_path.parent().unwrap_or(std::path::Path::new(""));
        fs::create_dir_all(dir)?;

        let pit_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.pit_path)?;
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.log_path)?;

        Ok(Self {
            config,
            pit_file: Mutex::new(pit_file),
            log_file: Mutex::new(log_file),
            seen_pids: Mutex::new(HashSet::new()),
        })
    }

    /// Record a process start. Returns true if it was intercepted.
    pub fn record(&self, pid: u32, ppid: u32, cmd: &str, args: &[String]) -> bool {
        // Check ignore list
        if self.config.ignore_list.contains(cmd) {
            return false;
        }

        let intercepted = self.config.intercept_enabled && self.config.intercept_list.contains(cmd);

        let cwd_str = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let cwd = cwd_str;

        let entry = PitEntry {
            timestamp: Utc::now().to_rfc3339(),
            pid,
            ppid,
            cmd: cmd.to_string(),
            args: args.to_vec(),
            cwd: cwd.clone(),
            intercepted,
            handler: if intercepted {
                Some(match cmd {
                    "git" => "gix".into(),
                    "gh" => "octocrab".into(),
                    "make" => "make-rs".into(),
                    "curl" => "reqwest".into(),
                    _ => "native".into(),
                })
            } else {
                None
            },
        };

        // Write JSONL to PIT file
        if let Ok(json) = serde_json::to_string(&entry) {
            if let Ok(mut f) = self.pit_file.lock() {
                let _ = writeln!(f, "{json}");
                let _ = f.flush();
            }
        }

        // Write human-readable log
        let inter = if intercepted { "[INTERCEPT]" } else { "[PASS]" };
        let log_line = format!(
            "{} {} pid={} ppid={} cmd={} cwd={}\n",
            inter, entry.timestamp, pid, ppid, cmd, cwd
        );
        if let Ok(mut f) = self.log_file.lock() {
            let _ = f.write_all(log_line.as_bytes());
            let _ = f.flush();
        }

        intercepted
    }

    /// Mark a PID as seen (dedup)
    pub fn mark_seen(&self, pid: u32) -> bool {
        let mut seen = self.seen_pids.lock().unwrap();
        seen.insert(pid)
    }

    /// Load existing PIDs from /proc (Linux)
    #[cfg(target_os = "linux")]
    pub fn scan_proc(&self) -> Vec<(u32, u32, String, Vec<String>)> {
        let mut out = Vec::new();
        let proc = match fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return out,
        };
        for entry in proc.flatten() {
            let name = entry.file_name();
            let pid: u32 = match name.to_str().and_then(|s| s.parse().ok()) {
                Some(p) => p,
                None => continue,
            };
            if !self.mark_seen(pid) {
                continue; // already seen
            }
            // Read cmdline
            let cmdline_path = entry.path().join("cmdline");
            let cmdline = match fs::read(&cmdline_path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let parts: Vec<&[u8]> = cmdline.splitn(2, |&b| b == 0).collect();
            if parts.is_empty() || parts[0].is_empty() {
                continue;
            }
            let cmd = String::from_utf8_lossy(parts[0]).to_string();
            let args: Vec<String> = cmdline
                .split(|&b| b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .collect();
            // Read ppid from status
            let ppid = Self::read_ppid(&entry.path());
            out.push((pid, ppid, cmd, args));
        }
        out
    }

    #[cfg(target_os = "linux")]
    fn read_ppid(proc_path: &std::path::Path) -> u32 {
        let status = match fs::read_to_string(proc_path.join("status")) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        for line in status.lines() {
            if let Some(val) = line.strip_prefix("PPid:\t") {
                return val.trim().parse().unwrap_or(0);
            }
        }
        0
    }

    /// High-level: scan /proc, record all new processes
    pub fn scan_and_record(&self) -> usize {
        #[cfg(target_os = "linux")]
        {
            let procs = self.scan_proc();
            let count = procs.len();
            for (pid, ppid, cmd, args) in procs {
                self.record(pid, ppid, &cmd, &args);
            }
            count
        }
        #[cfg(not(target_os = "linux"))]
        {
            0
        }
    }

    pub fn pit_path(&self) -> &PathBuf {
        &self.config.pit_path
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.config.log_path
    }
}

// ── Background watcher ────────────────────────────────────────────

pub async fn run_pit_watcher(config: PitConfig) {
    let pit = Pit::new(config).expect("PIT init");
    tracing::info!("PIT watcher started — {}", pit.pit_path().display());

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        interval.tick().await;
        let count = pit.scan_and_record();
        if count > 0 {
            tracing::debug!("PIT: {} new processes", count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pit_config_default() {
        let cfg = PitConfig::default();
        assert!(cfg.ignore_list.contains("bash"));
        assert!(cfg.intercept_list.contains("git"));
        assert!(cfg.intercept_enabled);
    }

    #[test]
    fn pit_record_ignored() {
        let dir = std::env::temp_dir().join("pit-test");
        let cfg = PitConfig {
            pit_path: dir.join("pit.jsonl"),
            log_path: dir.join("pit.log"),
            ..Default::default()
        };
        let pit = Pit::new(cfg).unwrap();
        let intercepted = pit.record(100, 1, "bash", &[]);
        assert!(!intercepted, "bash is in ignore list");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pit_record_intercepted() {
        let dir = std::env::temp_dir().join("pit-test-2");
        let cfg = PitConfig {
            pit_path: dir.join("pit.jsonl"),
            log_path: dir.join("pit.log"),
            ..Default::default()
        };
        let pit = Pit::new(cfg).unwrap();
        let intercepted = pit.record(200, 1, "git", &["status".into()]);
        assert!(intercepted, "git is in intercept list");
        // Verify file was written
        let content = std::fs::read_to_string(pit.pit_path()).unwrap();
        assert!(content.contains("git"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
