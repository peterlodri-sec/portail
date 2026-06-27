# Sentinel & OpenCode MCP Integration Design Spec

**Date:** 2026-06-27
**Status:** Approved design
**Scope:** Nix + Nushell environment integration with Portail's python-mcp sidecar and Sentinel watchdog verification.

---

## 1. Understanding Summary
* **What is being built**: An automated integration between the OpenCode multiplexer dev shell (Nix/Nushell), Portail's MCP servers, and the Sentinel health watchdog.
* **Why it exists**: To make sure Portail's MCP sidecar processes start cleanly, register automatically with OpenCode, and successfully route events through the system event log in a reproducible development environment.
* **Who it is for**: Developers working on Portail who need a reliable, zero-overhead environment to build and test agent integrations.
* **Key constraints**:
  * Managed entirely within Nix (`devShells.opencode-mux`) and Nushell (`ohmy-slim.nu`).
  * Binding strictly to `localhost` with dynamically allocated ports.
  * Rapid validation (under 3 seconds execution time).
* **Explicit non-goals**:
  * Modifying any external orchestration systems (like Kubernetes, systemd, or global macOS plist daemons) outside the local shell.
  * Modifying production deployment scripts (this is purely a dev shell feature).

---

## 2. Assumptions
1. **Zellij & Nushell integration**: We will build on top of the approved Zellij pane/tab layout structure defined in the existing `opencode-mux-integration` spec.
2. **Dynamic discovery**: OpenCode's configuration will be automatically updated with local MCP addresses at launch time without developer manual editing.
3. **Graceful cleanup**: Orphaned background sidecars will be terminated automatically if startup tests fail.

---

## 3. Decision Log
* **Decision**: Use "Caveman" (Nushell-driven background spawning) instead of Rust Supervisor-level sidecar management.
  * **Alternatives considered**: Modifying the Rust `Supervisor` to manage the Python process.
  * **Why chosen**: Keeps the core codebase clean of developer-only environment configuration, matches the lightweight design philosophy, and is easy to debug.
* **Decision**: Sentinel detects the event and registers verification state.
  * **Alternatives considered**: Having the test script poll the Python socket directly.
  * **Why chosen**: Validates the entire routing path (Client -> MCP -> Portail HTTP server -> EventLog -> Sentinel Watchdog) rather than just a process socket check.

---

## 4. Final Design

### 4.1 Process Spawning
The Nushell module (`nushell/ohmy-slim.nu`) will provide an automated function to spawn the Python MCP server process in the background. It will capture the PID and ensure it is cleaned up using an exit hook.

### 4.2 Config Sync
The launcher will dynamically write the local server connection details to `~/.config/opencode/mcp.json` to allow OpenCode to auto-discover the sidecar.

### 4.3 Validation Routing
1. Nushell launcher posts a `"HELLO"` event to the `/events` endpoint of Portail.
2. Portail logs this event.
3. Sentinel checks the event log, processes the `"HELLO"`, and registers `sentinel_hello_success` in the logs.
4. The launcher verifies the success and starts the multiplexer shell.
