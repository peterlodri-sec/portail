# nushell/ohmy-slim.nu — launchers for oh-my-opencode-slim
# Usage: use ohmy-slim.nu *

# ── Constants ─────────────────────────────────────────────────────

const CONFIG_PATH = "~/.config/opencode/oh-my-opencode-slim.json"
const PLUGIN_DIR = "/Users/lodripeter/.config/opencode/plugins/oh-my-opencode-slim"
const MIMALLOC_DARWIN = "/opt/homebrew/lib/libmimalloc.dylib"
const MULTIPLEXER_BLOCK = {
    type: "zellij"
    layout: "main-vertical"
    zellij_pane_mode: "agent-tab"
}

# ── Helpers ───────────────────────────────────────────────────────

# Pick a random high port in 49152..65535.
export def "ohmy-slim pick-port" [] {
    let min = 49152
    let max = 65535
    random int $min..=$max
}

# Deep-merge the multiplexer block into an existing config.
# Preserves every other top-level key byte-for-byte.
export def "ohmy-slim write-config" [
    --path: string = ""
] {
    let target = if ($path | is-empty) { $CONFIG_PATH | path expand } else { $path | path expand }
    let parent = ($target | path dirname)
    if not ($parent | path exists) {
        mkdir $parent
    }
    if not ($target | path exists) {
        { multiplexer: $MULTIPLEXER_BLOCK } | to json --indent 2 | save --force $target
        print $"wrote new config: ($target)"
        return
    }
    let raw = (open --raw $target)
    let parsed = (try { $raw | from json } | complete)
    if $parsed.exit_code != 0 {
        let backup = $"($target).bak.((date now | format date '%s'))"
        mv $target $backup
        { multiplexer: $MULTIPLEXER_BLOCK } | to json --indent 2 | save --force $target
        print $"WARN config unparseable, backed up to ($backup) and rewrote from template"
        return
    }
    let existing = $parsed.stdout
    let merged = ($existing | merge { multiplexer: ($existing.multiplexer? | default {} | merge $MULTIPLEXER_BLOCK) })
    $merged | to json --indent 2 | save --force $target
    print $"merged multiplexer block into: ($target)"
}

# ── Launchers ─────────────────────────────────────────────────────

# Standard launcher: background subagents + bun cache + node heap tuning.
export def "ohmy-slim launch" [
    --port: int = 4096
    --extra: list<string> = []
] {
    let extra_args = if ($extra | is-empty) { [] } else { $extra }
    $env.OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS = "true"
    $env.OH_MY_OPENCODE_SLIM_DIR = $PLUGIN_DIR
    $env.NODE_OPTIONS = "--max-old-space-size=4096 --optimize-for-size --expose-gc"
    $env.UV_THREADPOOL_SIZE = "4"
    $env.BUN_RUNTIME_TRANSPILER_CACHE = "1"
    $env.BUN_CACHE_DIR = "/tmp/bun-cache-ohmyopencode"
    if not ($env.BUN_CACHE_DIR | path exists) {
        mkdir $env.BUN_CACHE_DIR
    }
    let port_arg = $"--port=($port)"
    ^opencode ...$extra_args $port_arg
}

# ULTRA launcher: mimalloc + page-warm + QoS.
export def "ohmy-slim ultra" [
    --port: int = 4096
    --no-preload
] {
    $env.OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS = "true"
    $env.OH_MY_OPENCODE_SLIM_DIR = $PLUGIN_DIR
    $env.NODE_OPTIONS = "--max-old-space-size=2048 --optimize-for-size --expose-gc --no-lazy"
    $env.UV_THREADPOOL_SIZE = "4"
    $env.BUN_RUNTIME_TRANSPILER_CACHE = "1"
    $env.BUN_CACHE_DIR = "/tmp/bun-cache-ohmyopencode"
    if not ($env.BUN_CACHE_DIR | path exists) {
        mkdir $env.BUN_CACHE_DIR
    }

    if (sys host).name == "Darwin" {
        if ($MIMALLOC_DARWIN | path exists) {
            $env.DYLD_INSERT_LIBRARIES = $MIMALLOC_DARWIN
            $env.MIMALLOC_PURGE_DELAY = "500"
            $env.MIMALLOC_EAGER_COMMIT = "1"
            $env.MIMALLOC_OS_RESET_DELAY = "1000"
            $env.MIMALLOC_SHOW_ERRORS = "0"
        }
    }

    if not $no_preload {
        try {
            if ($CONFIG_PATH | path expand | path exists) {
                open --raw ($CONFIG_PATH | path expand) | bytes at 0..0 | ignore
            }
        }
    }

    try {
        ^nice -n -10 $env.PID o+e>| ignore
    }

    let binary = ([$PLUGIN_DIR "ohmy-slim-native"] | path join)
    let cmd = if ($binary | path exists) { $binary } else { "opencode" }
    ^$cmd ...["--port", ($port | into string)]
}

# Multiplexer launcher: write config + start zellij + run opencode.
export def "ohmy-slim mux-launch" [
    --port: int = 0
    --layout: string = "main-vertical"
] {
    if not (which zellij | is-not-empty) {
        print $"WARN zellij not on PATH, falling back to 'ohmy-slim launch'"
        ohmy-slim launch
        return
    }
    if (not ($env.TERM_PROGRAM? | default "") | is-empty) and ($env.ZELLIJ? | default "") != "" {
        print $"WARN already inside a zellij session, refusing to nest"
        ohmy-slim launch
        return
    }
    ohmy-slim write-config
    let actual_port = if $port == 0 { ohmy-slim pick-port } else { $port }
    $env.OPENCODE_PORT = ($actual_port | into string)
    let layout_arg = $"--layout=($layout)"
    ^zellij --new-session-with-layout default $layout_arg -- \
        ^nu -c $'use ohmy-slim.nu *; ohmy-slim launch --port ($actual_port)'
}

# ── Portail MCP & Sentinel Integration ─────────────────────────────

# Spawn Portail MCP server dynamically in the background.
export def "ohmy-slim spawn-mcp-servers" [
    --socket-path: string = "/tmp/portail-mcp.sock"
] {
    let python_mcp_dir = "/Users/lodripeter/workspace/peterlodri-sec/portail/plugins/portail-mcp"
    if not ($python_mcp_dir | path exists) {
        error make {msg: $"Portail MCP plugin directory not found at ($python_mcp_dir)"}
    }
    
    # Clean up stale socket file
    if ($socket_path | path exists) {
        rm --force $socket_path
    }
    
    print $"Spawning Python MCP server in background over socket: ($socket_path)"
    
    let python_cmd = $"import sys; sys.path.insert(0, '($python_mcp_dir)'); from portail_mcp.server import main; main()"
    let log_file = "/tmp/portail-mcp-sidecar.log"
    
    # Launch background process
    bash -c $"PYTHONPATH=($python_mcp_dir) python3 -c \"($python_cmd)\" --socket ($socket_path) > ($log_file) 2>&1 & echo $!" | trim | save --force "/tmp/portail-mcp-sidecar.pid"
    
    # Wait briefly for socket creation
    for i in 1..20 {
        if ($socket_path | path exists) {
            break
        }
        sleep 100ms
    }
    
    if not ($socket_path | path exists) {
        error make {msg: $"Python MCP server failed to start or create socket at ($socket_path) within timeout"}
    }
    
    let pid = (open "/tmp/portail-mcp-sidecar.pid" | into int)
    print $"Spawning completed successfully. PID: ($pid)"
    return $pid
}

# Execute a HELLO integration test on Portail events endpoint.
export def "ohmy-slim run-hello-test" [
    --portail-port: int = 8787
] {
    let url = $"http://localhost:($portail_port)/events"
    print $"Sending HELLO event to ($url)"
    
    let payload = {
        agent_id: "test"
        event_type: "HELLO"
        severity: "info"
        timestamp: 0
        metadata: {}
    }
    
    let response = (http post -t application/json $url $payload)
    print $"Response received: ($response)"
    
    # Monitor events to find the verification success log
    print "Waiting for Sentinel confirmation event..."
    for i in 1..20 {
        let events = (http get $url)
        let found = ($events | where event_type == "sentinel_hello_success" | is-not-empty)
        if $found {
            print $"[OK] Sentinel successfully verified the HELLO handshake!"
            return true
        }
        sleep 100ms
    }
    
    error make {msg: "Sentinel HELLO verification timed out"}
}
