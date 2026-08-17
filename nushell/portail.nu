# portail/portail.nu — CLI wrappers + fleet ops for Portail
# Usage: use portail.nu

const BENCH_HOST = "bench-node"
const BENCH_IP = "178.105.245.135"
const PORTAIL_SERVICE = "portail-staging"
const HEALTH_URL = "http://localhost:8787/health"
const REMOTE_BINARY = "/opt/portail-staging/target/release/portail"

# NOTE on performance: every `portail *` wrapper below dispatches through `cargo run --release`.
# cargo run recompiles only when sources changed (incremental), but still pays link + exec
# startup each call. For heavy interactive use, `cargo build --release` once and set
# $env.PORTAIL_RUN = "./target/release/portail" to route the wrappers through the prebuilt
# binary instead of cargo (fast, but you are responsible for keeping it fresh).

# Resolve the command used to launch the portail CLI.
# Default is `cargo run --release` (recompiles on source change, so always correct).
# Set $env.PORTAIL_RUN to a prebuilt binary path to skip the cargo layer entirely.
def portail-cmd [] {
    let prebuilt = ($env.PORTAIL_RUN? | default null)
    if $prebuilt != null and ($prebuilt | path exists) {
        { bin: $prebuilt, args: [] }
    } else {
        { bin: "cargo", args: ["run", "--release", "--"] }
    }
}

# ── Server ────────────────────────────────────────────────────────

# NOTE: `serve` has no `--port` flag; the listen address is set by `listen` in the config file.
def "portail serve" [
    --config (-c): string = "portail.toml"
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args --config $config serve
}

# ── Status ────────────────────────────────────────────────────────

def "portail status" [] {
    let p = (portail-cmd)
    let raw = (^$p.bin ...$p.args status | complete)
    if $raw.exit_code != 0 {
        print $"(ansi red_bold)portail status failed(ansi reset)"
        print $raw.stderr
        return
    }
    $raw.stdout | lines | each { |line|
        let parts = ($line | split row ": ")
        if ($parts | length) >= 2 {
            { key: ($parts | first | str trim), value: ($parts | skip 1 | str join ": " | str trim) }
        } else {
            { key: "output", value: ($line | str trim) }
        }
    } | table --index false
}

# ── Health ────────────────────────────────────────────────────────

def "portail health" [
    --url (-u): string = "http://localhost:8787"
] {
    try {
        http get $"($url)/health" --max-time 3sec | table --index false
    } catch {
        print $"(ansi red_bold)Health check failed(ansi reset)"
        print $in
    }
}

# ── Doctor ────────────────────────────────────────────────────────

def "portail doctor" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args doctor
}

# ── Events ────────────────────────────────────────────────────────

def "portail events" [
    --count (-n): int = 20
    --stream (-s)
] {
    let p = (portail-cmd)
    if $stream {
        ^$p.bin ...$p.args events --stream
    } else {
        ^$p.bin ...$p.args events --count $count
    }
}

# ── Config ────────────────────────────────────────────────────────

def "portail config show" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args config show
}

def "portail config validate" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args config validate
}

# ── Hooks ─────────────────────────────────────────────────────────

def "portail hooks list" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args hooks list
}

# `hooks add` takes a full hook record as JSON (see `portail hooks show <id>` for the shape).
def "portail hooks add" [
    hook: string
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args hooks add --hook $hook
}

def "portail hooks show" [
    id: string
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args hooks show $id
}

def "portail hooks delete" [
    id: string
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args hooks delete $id
}

# ── Cache ─────────────────────────────────────────────────────────

def "portail cache stats" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args cache stats
}

def "portail cache purge" [
    prefix: string
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args cache purge $prefix
}

def "portail cache ratio" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args cache ratio
}

# ── Targets ───────────────────────────────────────────────────────

def "portail target list" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args target list
}

def "portail target builtins" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args target builtins
}

def "portail target export" [
    name: string
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args target export $name
}

# ── MCP ───────────────────────────────────────────────────────────

def "portail mcp list" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args mcp list
}

def "portail mcp builtins" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args mcp builtins
}

def "portail mcp info" [
    name: string
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args mcp info $name
}

# ── Drift / Spec / Fuzz ──────────────────────────────────────────

def "portail drift-detect" [
    --url: string = "http://localhost:8787"
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args drift-detect capture --url $url
}

def "portail drift-replay" [
    --url: string = "http://localhost:8787"
] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args drift-detect replay --url $url
}

def "portail spec-generate" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args spec-verify generate
}

def "portail spec-check" [] {
    let p = (portail-cmd)
    ^$p.bin ...$p.args spec-verify check
}

# ── Fleet: probe ──────────────────────────────────────────────────

def "portail probe" [] {
    let service = ($env.PORTAIL_SERVICE? | default $PORTAIL_SERVICE)
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
        { check: $check.label, status: status, output: output }
    } | table --index false
}

# ── Fleet: deploy ─────────────────────────────────────────────────

def "portail deploy" [] {
    let service = ($env.PORTAIL_SERVICE? | default $PORTAIL_SERVICE)

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

# ── Fleet: drift ──────────────────────────────────────────────────

def "portail drift" [] {
    let local_version = (open Cargo.toml | get package.version)
    let remote_toml = (do -i {
        ssh $BENCH_HOST $"grep '^version' /opt/portail-staging/Cargo.toml | head -1"
    } | complete | get stdout | str trim)
    let remote_version = ($remote_toml | str replace --all --regex 'version\s*=\s*"' "" | str replace --all '"' "")

    let match = $local_version == $remote_version
    let verdict = if $match {
        $"(ansi green_bold)versions match(ansi reset)"
    } else {
        $"(ansi red_bold)DRIFT(ansi reset)"
    }

    { local: $local_version, remote: $remote_version, verdict: $verdict } | table --index false
}

# ── Fleet: restart ────────────────────────────────────────────────

def "portail restart" [] {
    let service = ($env.PORTAIL_SERVICE? | default $PORTAIL_SERVICE)
    print $"(ansi cyan_bold)── Restarting ($service) on ($BENCH_HOST) ──(ansi reset)"
    ssh $BENCH_HOST $"sudo systemctl restart ($service)"
    if $env.LAST_EXIT_CODE == 0 {
        print $"(ansi green_bold)Service restarted.(ansi reset)"
    } else {
        print $"(ansi red_bold)Restart failed.(ansi reset)"
    }
}

# ── Fleet: logs ───────────────────────────────────────────────────

def "portail logs" [
    --lines (-n): int = 80
] {
    let service = ($env.PORTAIL_SERVICE? | default $PORTAIL_SERVICE)
    ssh $BENCH_HOST $"journalctl -u ($service) -f --no-pager -n ($lines)"
}

# ── Fleet: bench status ──────────────────────────────────────────

def "portail bench" [] {
    let service = ($env.PORTAIL_SERVICE? | default $PORTAIL_SERVICE)
    let cmd = $"echo '--- load ---' && cat /proc/loadavg && echo '--- memory ---' && free -h | head -2 && echo '--- disk ---' && df -h / | head -2 && echo '--- process ---' && systemctl is-active ($service)"
    let result = (do -i { ssh $BENCH_HOST $cmd } | complete)
    if $result.exit_code == 0 {
        print $result.stdout
    } else {
        print $"(ansi red_bold)bench-status failed: ($result.stderr)(ansi reset)"
    }
}
