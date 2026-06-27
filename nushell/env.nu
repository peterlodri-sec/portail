# portail/env.nu — project-specific nushell environment
# Sourced by: source env.nu (from project root)

# ── Paths ──────────────────────────────────────────────────────
$env.CARGO_TARGET_DIR = ($env.PWD | path join "target")
$env.RUST_LOG = "portail=info,tower_http=info"

# ── Aliases ────────────────────────────────────────────────────
alias pg = cargo run --release --
alias pt = cargo test
alias pc = cargo check
alias pb = cargo build --release
alias lint = cargo clippy -- -D warnings
alias fmt = cargo fmt

# ── Portail shortcuts ──────────────────────────────────────────
alias serve = cargo run --release -- serve
alias status = cargo run --release -- status
alias doctor = cargo run --release -- doctor
alias init = cargo run --release -- init
alias config-show = cargo run --release -- config show
alias hooks-list = cargo run --release -- hooks list
alias events-tail = cargo run --release -- events --tail

# ── Staging ────────────────────────────────────────────────────
alias staging-ssh = ssh bench-node
alias staging-build = ssh bench-node 'source ~/.cargo/env && cd /opt/portail-staging && cargo build --release'
alias staging-status = ssh bench-node 'systemctl status portail-staging'
alias staging-logs = ssh bench-node 'journalctl -u portail-staging -f'
alias staging-health = ssh bench-node 'curl -s http://localhost:8787/health'

# ── Development helpers ────────────────────────────────────────
alias watch-build = watch ./src/ { |op, path, new_path| if $op == "Write" { print $"($op) ($path)"; cargo check } }
alias test-w = watch ./src/ { |op, path, new_path| if $op == "Write" { print $"($op) ($path)"; cargo test } }

# ── Quick health check ────────────────────────────────────────
def health [] {
    http get "http://localhost:8787/health" | table
}

# ── Print config on enter ─────────────────────────────────────
print $"(ansi green_bold)Portail dev shell loaded (ansi reset) — (ansi cyan)($env.PWD)(ansi reset)"
print $"  fleet: use fleet.nu  |  cli wrappers: use portail.nu"
