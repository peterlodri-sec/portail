# Portail V4 — VKID Integrity Kernel + Built-in Services

**Target:** v4.0.0
**Status:** Planning

---

## 1. VKID (Vaked Integrity Kernel) Integration

Vaked is a deterministic agentic swarm appliance engineered for absolute execution
determinism, low-latency performance, and immutable system integrity.

### Architecture Layers

```
┌──────────────────────────────────────────────────────────┐
│  Portail Agent Runloops (ADK-Rust, WASM sandboxes)       │
├──────────────────────────────────────────────────────────┤
│  Memory Plane: Zero-Copy Shared Memory (mmap)            │
├──────────────────────────────────────────────────────────┤
│  VKID Root Integrity Kernel: seccomp BPF, process guard  │
├──────────────────────────────────────────────────────────┤
│  Host Layer: Ephemeral NixOS tmpfs or Talos Linux        │
└──────────────────────────────────────────────────────────┘
```

### Integration Points

| Portail Component | VKID Equivalent | Integration |
|------------------|-----------------|-------------|
| Supervisor (respawn) | Root Integrity Kernel | Replace process supervision with VKID's seccomp-BPF guarded lifecycle |
| PIT (process tracker) | Genesis attestation | PIT logs feed into VKID's continuous hash verification |
| Release-audit | Genesis Seal | Release pipeline produces GENESIS_SEAL.hash notarized in DNS |
| Config (figment) | Immutable config domain | Figment extract() at entry, immutable refs passed to runloops |
| Portal MCP | WASM sandbox with capability I/O | Replace ptrace-based MCP sidecar with WASM sandbox (Wasmtime) |
| Target templates | Capability graph | Provider targets become typed capabilities with proof chains |

### Genesis Ceremony

```nix
# flake.nix postInstall — already implemented
postInstall = ''
  mkdir -p $out/var/portail
  sha256sum $out/bin/portail > $out/var/portail/GENESIS_SEAL.hash
'';
```

**Phase 2:** Notarize GENESIS_SEAL.hash into DNS TXT record at
`_vaked.portail.dev` for continuous remote attestation.

---

## 2. Built-in Services

### BOW (Backend Object Warehouse) — Secret & Identity Management

Purpose: Agent-accessible secret storage, API key distribution, identity tokens.
Replaces: HashiCorp Vault, 1Password CLI, manual .env management.

```rust
// Design sketch
pub struct BowConfig {
    pub store_path: PathBuf,        // encrypted SQLite store
    pub auto_unlock: bool,          // unlock via TPM/enclave
    pub audit_log: bool,            // log every secret access
}

pub enum BowSecret {
    ApiKey { name: String, value: Encrypted, provider: String },
    EnvVar { name: String, value: Encrypted },
    Identity { issuer: String, credential: Vec<u8> },
}
```

**CLI:** `portail bow set <name> <value>`, `portail bow get <name>`,
`portail bow list`, `portail bow rotate <name>`

### CREPSC (Codebase Retrieval, Exploration & Semantic Query)

MCP server for semantic code search over the local codebase.
Combines: ripgrep for text search + tree-sitter for AST + vector embeddings.

### Mercury — Message/Event Bus Bridge

Bridges between NATS (existing), MQTT, and in-memory event log.
Agents publish/subscribe across protocols transparently.

### Sentinel-X — Extended Health & Observability

Upgrades the existing sentinel to:
- Prometheus remote write
- Structured audit log export (OpenTelemetry)
- Automated dashboard provisioning

### Maelstrom — Chaos Engineering Agent

Deliberately injects faults (packet loss, process kill, disk latency)
into the system and verifies the supervisor auto-recovers.

---

## 3. Capability Graph Language

Replace unstructured provider/plugin config with a typed capability graph.

### Concept

Each capability (target, MCP server, plugin) is a node in a DAG:

```
target:anthropic-fast
  ├─ provider: anthropic
  ├─ models: [claude-sonnet-4, claude-haiku-3]
  ├─ rps: 10
  └─ requires: [bow:anthropic-api-key]

mcp:filesystem
  ├─ transport: stdio
  ├─ command: npx @modelcontextprotocol/server-filesystem
  └─ capabilities: [fs:read, fs:write, fs:search]

capability:fs:read
  ├─ grants: [path:/src, path:/docs]
  └─ audited: true
```

### Lowering

The graph is "lowered" at boot into concrete config:

```rust
// High-level capability (developer writes this)
capability "deploy-production" {
    uses target "anthropic-fast"
    uses mcp "filesystem" { paths = ["/src"] }
    uses mcp "github"
}

// Lowered to (compiler generates this)
[[targets]]
name = "anthropic-fast"
provider = "anthropic"
base_url = "https://api.anthropic.com/v1"
models = ["claude-sonnet-4"]

[[mcp.server_registry]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/src"]

[[mcp.server_registry]]
name = "github"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

### Rust lowerer

```rust
pub enum Capability {
    Target(TargetCapability),
    Mcp(McpCapability),
    Bow(BowCapability),
}

pub struct CapabilityGraph {
    nodes: Vec<Capability>,
    edges: Vec<(usize, usize)>, // depends-on relationships
}

impl CapabilityGraph {
    pub fn lower(&self) -> Config {
        // Walk DAG, collect Config struct
    }
    pub fn verify(&self) -> Result<(), Vec<String>> {
        // Check all deps satisfied, no cycles
    }
}
```

---

## 4. Roadmap to V4

| Phase | What | Depends On |
|-------|------|-----------|
| P0 | Figment config (done) | — |
| P1 | Built-in MCP servers (done) | — |
| P2 | Target templates (done) | — |
| P3 | BOW secret management | figment config |
| P4 | Deep research CI agent | search APIs |
| P5 | Capability graph language | target + MCP + BOW |
| P6 | VKID Genesis attestation | release pipeline |
| P7 | WASM sandbox for MCP sidecar | capability graph |

---

## 5. Modern CLI Layer — No Legacy Tools

The `portail` CLI and all dev shell environments MUST use only modern,
actively maintained tools. Legacy tools (grep, awk, sed, cat, ls, find)
are banned in scripts, Taskfile, and CI.

| Legacy | Modern Replacement | Why |
|--------|-------------------|-----|
| `grep` | `rg` (ripgrep) | 5-10x faster, gitignore-aware, JSON output |
| `awk` | `jq` + `rg` | Structured data, composable, no Turing-complete DSL |
| `sed` | `sd` (sed alternative) | Human-readable regex, in-place, JSON-aware |
| `cat` | `bat` or `< file` | Syntax highlighting, git integration |
| `ls` | `eza` / `exa` | Colors, icons, tree view, git status |
| `find` | `fd` | 9x faster, intuitive syntax, .gitignore-aware |
| `du` | `dua` / `dust` | Interactive, visual, faster |
| `top` | `btm` (bottom) | GPU, network, graph view |
| `diff` | `delta` / `difftastic` | Syntax-highlighted side-by-side |
| `curl` | `httpie` or native `reqwest` | Structured JSON output, sessions |
| `bash` | `nushell` / `zsh` + `fish` | Structured data pipes, typed values |
| `tmux` | `zellij` | Built-in UI, floating panes, session mgmt |
| `ssh` | `mosh` + `ssh` | Roaming, predictive echo, UDP transport |
| `ping` | `gping` | Graph + histogram, cross-platform |

### Enforced via dev shell

```nix
# In flake.nix devShell
nativeBuildInputs = with pkgs; [
  ripgrep jq sd bat eza fd dua bottom delta
  zellij gping httpie doggo hyperfine just
];
```

### Taskfile aliases (no legacy commands)

```yaml
tasks:
  search:   'rg {.PATTERN} src/'
  fmt-json: 'sd "  " "    " **/*.json'
  tree:     'eza --tree --git-ignore'
  du:       'dua interactive'
  stats:    'dust src/'
  top:      'btm'
```

## 6. Built-in MCP Servers (shipped)

| Name | What | Context Savings |
|------|------|-----------------|
| filesystem | Read/write/search files | Replaces cat/ls tool calls (~95%) |
| github | PRs, issues, search | Replaces gh CLI subprocess (~80%) |
| playwright | Chrome DevTools, browser | Replaces screenshot dumps (~90%) |
| fetch | HTTP download | Replaces curl subprocess (~70%) |
| brave-search | Web search | Replaces raw search JSON (~85%) |
| sqlite | Database queries | Replaces raw SQL dumps (~90%) |
| sequential-thinking | Reasoning chains | Replaces rambling CoT (~60%) |
