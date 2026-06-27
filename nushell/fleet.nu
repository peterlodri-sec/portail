# portail/fleet.nu — Fleet management functions for Portail staging
# Source with: use fleet.nu

# ── Constants ──────────────────────────────────────────────────
const BENCH_HOST = "bench-node"
const BENCH_IP = "178.105.245.135"
const PORTAIL_SERVICE = "portail-staging"
const HEALTH_URL = "http://localhost:8787/health"
const REMOTE_BINARY = "/opt/portail-staging/target/release/portail"

# ── probe-all ──────────────────────────────────────────────────
# SSH into bench-node and check: service status, disk, load, memory.
def probe-all [] {
    let service = (env PORTAIL_SERVICE | default "portail-staging")
    let checks = [
        { label: "service", cmd: $"systemctl is-active ($service)" }
        { label: "health",  cmd: $"curl -sf ($HEALTH_URL)" }
        { label: "disk",    cmd: "df -h / | tail -1" }
        { label: "load",    cmd: "cat /proc/loadavg" }
        { label: "memory",  cmd: "free -h | awk '/^Mem:/{print $3\"/\"$2\" used, \" $4\" free\"}'" }
    ]

    print $"(ansi cyan_bold)── Probing (ansi yellow_bold)($BENCH_HOST)(ansi cyan_bold) ──(ansi reset)\n"

    $checks | par-each { |check|
        let result = (do -i { ssh $BENCH_HOST $check.cmd } | complete)
        let status = if $result.exit_code == 0 { (ansi green_bold) + "ok" } else { (ansi red_bold) + "fail" }
        let output = if $result.stdout != "" { $result.stdout | str trim } else { $result.stderr | str trim }
        { check: $check.label, status: status, output: $output }
    } | table --index false
}

# ── deploy-staging ─────────────────────────────────────────────
# Build locally, rsync binary to bench-node, restart service.
def deploy-staging [] {
    let service = (env PORTAIL_SERVICE | default "portail-staging")

    print $"(ansi cyan_bold)── Building release ──(ansi reset)"
    cargo build --release
    if $env.LAST_EXIT_CODE != 0 {
        print $"(ansi red_bold)Build failed.(ansi reset)"
        return
    }

    let bin = ([$env.PWD, "target", "release", "portail"] | path join)
    if not ($bin | path exists) {
        print $"(ansi red_bold)Binary not found at ($bin)(ansi reset)"
        return
    }

    print $"(ansi cyan_bold)── Rsyncing to ($BENCH_HOST) ──(ansi reset)"
    rsync -azP --chmod=+x $bin $"($BENCH_HOST):($REMOTE_BINARY)"
    if $env.LAST_EXIT_CODE != 0 {
        print $"(ansi red_bold)Rsync failed.(ansi reset)"
        return
    }

    print $"(ansi cyan_bold)── Restarting ($service) ──(ansi reset)"
    ssh $BENCH_HOST $"sudo systemctl restart ($service)"
    if $env.LAST_EXIT_CODE == 0 {
        print $"(ansi green_bold)Deploy complete.(ansi reset)"
    } else {
        print $"(ansi red_bold)Restart failed.(ansi reset)"
    }
}

# ── logs-staging ───────────────────────────────────────────────
# Tail portail-staging logs on bench-node.
def logs-staging [
    --lines (-n): int = 80   # number of lines to show
] {
    let service = (env PORTAIL_SERVICE | default "portail-staging")
    ssh $BENCH_HOST $"journalctl -u ($service) -f --no-pager -n ($lines)"
}

# ── drift-check ────────────────────────────────────────────────
# Compare local Cargo.toml version vs deployed version on bench-node.
def drift-check [] {
    let local_version = (open Cargo.toml | get package.version)
    let remote_toml = (do -i {
        ssh $BENCH_HOST $"grep '^version' /opt/portail-staging/Cargo.toml | head -1"
    } | complete | get stdout | str trim)
    let remote_version = ($remote_toml | str replace --all 'version\s*=\s*"' "" | str replace --all '"' "")

    let match = $local_version == $remote_version
    let verdict = if $match {
        $"(ansi green_bold)versions match(ansi reset)"
    } else {
        $"(ansi red_bold)DRIFT DETECTED(ansi reset)"
    }

    {
        local: $local_version,
        remote: $remote_version,
        verdict: $verdict,
    } | table --index false
}

# ── restart-staging ────────────────────────────────────────────
# Restart portail-staging service on bench-node.
def restart-staging [] {
    let service = (env PORTAIL_SERVICE | default "portail-staging")
    print $"(ansi cyan_bold)── Restarting ($service) on ($BENCH_HOST) ──(ansi reset)"
    ssh $BENCH_HOST $"sudo systemctl restart ($service)"
    if $env.LAST_EXIT_CODE == 0 {
        print $"(ansi green_bold)Service restarted.(ansi reset)"
    } else {
        print $"(ansi red_bold)Restart failed.(ansi reset)"
    }
}

# ── bench-status ───────────────────────────────────────────────
# Quick one-liner: load, memory, disk, process status.
def bench-status [] {
    let service = (env PORTAIL_SERVICE | default "portail-staging")
    let cmd = $"echo '--- load ---' && cat /proc/loadavg && echo '--- memory ---' && free -h | head -2 && echo '--- disk ---' && df -h / | head -2 && echo '--- process ---' && systemctl is-active ($service)"
    let result = (do -i { ssh $BENCH_HOST $cmd } | complete)
    if $result.exit_code == 0 {
        print $result.stdout
    } else {
        print $"(ansi red_bold)bench-status failed: ($result.stderr)(ansi reset)"
    }
}
