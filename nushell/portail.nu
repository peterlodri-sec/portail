# portail/portail.nu — CLI wrappers + fleet ops for Portail
# Usage: use portail.nu

const BIN = "cargo run --release --"
const BENCH_HOST = "bench-node"
const BENCH_IP = "178.105.245.135"
const PORTAIL_SERVICE = "portail-staging"
const HEALTH_URL = "http://localhost:8787/health"
const REMOTE_BINARY = "/opt/portail-staging/target/release/portail"

# ── Server ────────────────────────────────────────────────────────

def "portail serve" [
    --port (-p): int = 8787
    --config (-c): string
] {
    let args = ["serve" $"--port=($port)"]
    let args = if $config != null { $args | merge [$"--config=($config)"] } else { $args }
    ^cargo run --release -- ...$args
}

# ── Status ────────────────────────────────────────────────────────

def "portail status" [] {
    let raw = (^cargo run --release -- status | complete)
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
    let result = (http get $"($url)/health" --timeout 3sec | complete)
    if $result.exit_code == 0 {
        $result.stdout | from json | table --index false
    } else {
        print $"(ansi red_bold)Health check failed(ansi reset)"
        print $result.stderr
    }
}

# ── Doctor ────────────────────────────────────────────────────────

def "portail doctor" [] {
    ^cargo run --release -- doctor
}

# ── Events ────────────────────────────────────────────────────────

def "portail events" [
    --tail (-t): int = 50
    --follow (-f)
] {
    if $follow {
        ^cargo run --release -- events --tail $tail --follow
    } else {
        let raw = (^cargo run --release -- events --tail $tail | complete)
        if $raw.exit_code != 0 {
            print $raw.stderr
            return
        }
        $raw.stdout | lines | each { |line|
            if ($line | str trim | is-empty) { null } else {
                { raw: ($line | str trim) }
            }
        } | compact | table --index false
    }
}

# ── Config ────────────────────────────────────────────────────────

def "portail config show" [] {
    ^cargo run --release -- config show
}

def "portail config validate" [] {
    ^cargo run --release -- config validate
}

# ── Hooks ─────────────────────────────────────────────────────────

def "portail hooks list" [] {
    ^cargo run --release -- hooks list
}

def "portail hooks create" [
    name: string
    --event: string = "request"
    --script: string
] {
    ^cargo run --release -- hooks create $name --event $event --script $script
}

def "portail hooks delete" [
    id: string
] {
    ^cargo run --release -- hooks delete $id
}

# ── Cache ─────────────────────────────────────────────────────────

def "portail cache stats" [] {
    ^cargo run --release -- cache stats
}

def "portail cache clear" [] {
    ^cargo run --release -- cache clear
}

# ── Targets ───────────────────────────────────────────────────────

def "portail target list" [] {
    ^cargo run --release -- target list
}

def "portail target add" [
    name: string
    --provider: string = "openai"
    --url: string
    --models: string
] {
    let args = ["target" "add" $name $"--provider=($provider)" $"--url=($url)"]
    let args = if $models != null { $args | merge [$"--models=($models)"] } else { $args }
    ^cargo run --release -- ...$args
}

# ── MCP ───────────────────────────────────────────────────────────

def "portail mcp status" [] {
    ^cargo run --release -- mcp status
}

def "portail mcp restart" [] {
    ^cargo run --release -- mcp restart
}

# ── Drift / Spec / Fuzz ──────────────────────────────────────────

def "portail drift-detect" [] {
    ^cargo run --release -- drift-detect
}

def "portail spec-verify" [] {
    ^cargo run --release -- spec-verify
}

def "portail fuzz-route" [
    --url: string = "http://localhost:8787"
] {
    ^cargo run --release -- fuzz-route --url $url
}

# ── Fleet: probe ──────────────────────────────────────────────────

def "portail probe" [] {
    let service = (env PORTAIL_SERVICE | default $PORTAIL_SERVICE)
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
    let service = (env PORTAIL_SERVICE | default $PORTAIL_SERVICE)

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
    let remote_version = ($remote_toml | str replace --all 'version\s*=\s*"' "" | str replace --all '"' "")

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
    let service = (env PORTAIL_SERVICE | default $PORTAIL_SERVICE)
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
    let service = (env PORTAIL_SERVICE | default $PORTAIL_SERVICE)
    ssh $BENCH_HOST $"journalctl -u ($service) -f --no-pager -n ($lines)"
}

# ── Fleet: bench status ──────────────────────────────────────────

def "portail bench" [] {
    let service = (env PORTAIL_SERVICE | default $PORTAIL_SERVICE)
    let cmd = $"echo '--- load ---' && cat /proc/loadavg && echo '--- memory ---' && free -h | head -2 && echo '--- disk ---' && df -h / | head -2 && echo '--- process ---' && systemctl is-active ($service)"
    let result = (do -i { ssh $BENCH_HOST $cmd } | complete)
    if $result.exit_code == 0 {
        print $result.stdout
    } else {
        print $"(ansi red_bold)bench-status failed: ($result.stderr)(ansi reset)"
    }
}
