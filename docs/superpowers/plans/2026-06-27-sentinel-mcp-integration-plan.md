# Sentinel & OpenCode MCP Integration Implementation Plan

This plan tracks the step-by-step implementation to wire Portail's Python MCP servers and Sentinel event verification into the OpenCode multiplexer dev shell environment.

---

## Task 1: Extend Nushell Launcher (`nushell/ohmy-slim.nu`)
- [ ] **Step 1**: Add helper command `ohmy-slim spawn-mcp-servers` to locate Portail's Python MCP package (`plugins/portail-mcp`), spawn the server process on a dynamic high port, and track the process ID (PID).
- [ ] **Step 2**: Add `ohmy-slim run-hello-test` to execute a `POST` request to Portail's local event log endpoint (`/events`) containing the `HELLO` validation payload.
- [ ] **Step 3**: Configure lifecycle traps in `ohmy-slim mux-launch` to automatically kill background MCP server PIDs upon exit.

---

## Task 2: Update Sentinel Watchdog (`src/sentinel/mod.rs`)
- [ ] **Step 1**: Update the 30-second tokio timer loop (or check immediately upon receiving requests) to detect `HELLO` events.
- [ ] **Step 2**: Once a valid `HELLO` event log entry is detected, publish a `sentinel_hello_success` event to the `EventLog`.

---

## Task 3: Flake & App Config Sync (`nix/opencode-mux.nix`)
- [ ] **Step 1**: Update the `devShells.opencode-mux` environment variables to make sure Python3 is available.
- [ ] **Step 2**: Ensure the Nix launcher scripts invoke the new Nushell validation routing before starting the main Zellij workspace sessions.

---

## Task 4: Validation & Smoke Testing
- [ ] **Step 1**: Run the end-to-end integration launcher sequence.
- [ ] **Step 2**: Assert that `sentinel_hello_success` is successfully logged.
- [ ] **Step 3**: Confirm background Python processes are successfully terminated when the dev shell exits.
