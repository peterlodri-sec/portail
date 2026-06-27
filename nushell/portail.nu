# portail/portail.nu — Nushell wrappers for the portail CLI
# Usage: use portail.nu
#
# Provides typed, composable nushell commands that wrap `cargo run --release --`
# with structured output. Fleet operations (probe, deploy, drift) use par-each.

const BIN = "cargo run --release --"

# ── Server ────────────────────────────────────────────────────────

def "portail serve" [
    --port (-p): int = 8787     # Listen port
    --config (-c): string        # Config file path
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
    --tail (-t): int = 50       # Number of events to show
    --follow (-f)               # Follow event stream
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
    let raw = (^cargo run --release -- hooks list | complete)
    if $raw.exit_code == 0 {
        $raw.stdout
    } else {
        print $raw.stderr
    }
}

def "portail hooks create" [
    name: string                 # Hook name
    --event: string = "request"  # Event type
    --script: string             # Script path
] {
    ^cargo run --release -- hooks create $name --event $event --script $script
}

def "portail hooks delete" [
    id: string                   # Hook ID
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
    --models: string             # Comma-separated model list
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

# ── Fleet operations (SSH into bench-node) ────────────────────────

def "portail probe" [] {
    use fleet.nu *
    probe-all
}

def "portail deploy" [] {
    use fleet.nu *
    deploy-staging
}

def "portail drift" [] {
    use fleet.nu *
    drift-check
}

def "portail restart" [] {
    use fleet.nu *
    restart-staging
}

def "portail logs" [
    --lines (-n): int = 80
] {
    use fleet.nu *
    logs-staging --lines $lines
}

def "portail bench" [] {
    use fleet.nu *
    bench-status
}
