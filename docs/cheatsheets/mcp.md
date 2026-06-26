# Portail Built-in MCP Servers — Cheatsheet

**Portail ships 7 built-in MCP server templates** that wire into the Python
sidecar on boot. Each one reduces context window bloat by letting agents
call tools instead of dumping raw data into prompts.

---

## Quick Reference

| Server | Transport | Command | Context Savings |
|--------|-----------|---------|-----------------|
| filesystem | stdio | `npx @modelcontextprotocol/server-filesystem` | Replaces `cat`/`ls` tool calls with structured file read |
| github | stdio | `npx @modelcontextprotocol/server-github` | Replaces raw API JSON with structured PR/issue data |
| playwright | stdio | `npx @playwright/mcp` | Replaces screenshot HTML dumps with live browser access |
| fetch | stdio | `npx @modelcontextprotocol/server-fetch` | Replaces `curl` shell calls with structured HTTP |
| brave-search | stdio | `npx @modelcontextprotocol/server-brave-search` | Replaces web search tool with structured results |
| sqlite | stdio | `npx @modelcontextprotocol/server-sqlite` | Replaces raw SQL dumps with structured queries |
| sequential-thinking | stdio | `npx @modelcontextprotocol/server-sequential-thinking` | Replaces rambling CoT with structured reasoning steps |

---

## CLI Commands

```bash
# List configured MCP servers
portail mcp list

# Show details of a specific server
portail mcp info filesystem
portail mcp info playwright

# Get a ready-to-use config block
portail mcp config brave-search

# List all built-in templates
portail mcp builtins
```

---

## How Context Is Saved

**Without MCP** (dumping everything inline):
```
User: "read src/main.rs and find the main function"
Agent: [cat src/main.rs → 500 lines of source text in prompt]
Agent: [grep for fn main → 200 lines of context]
Total tokens used: ~15,000
```

**With MCP filesystem server**:
```
User: "read src/main.rs and find the main function"
Agent: [tool_call: read_file(path="src/main.rs") → structured response]
       [tool_call: grep(pattern="fn main") → 3 line snippet]
Total tokens used: ~500
Savings: 96%
```

---

## Configuration

### In portail.toml
```toml
[mcp]
enabled = true
socket_path = "/run/portail/mcp.sock"

[[mcp.server_registry]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/"]
autostart = true
```

### Via CLI
```bash
# Generate config for any built-in server
portail mcp config playwright >> portail.toml
```

---

## Target Templates (Upstream Providers)

```bash
# List all targets (built-in + configured)
portail target list

# Export a target as JSON (paste into another portail.toml)
portail target export anthropic-fast

# Show built-in defaults
portail target builtins
```

### Default targets

| Name | Provider | Models | RPS |
|------|----------|--------|-----|
| anthropic-fast | anthropic | claude-sonnet-4, claude-haiku-3 | 10 |
| anthropic-smart | anthropic | claude-opus-4 | 5 |
| openai-gpt5 | openai | gpt-5.4, gpt-5.2, gpt-5.1 | 10 |
| openai-o-series | openai | o3, o4-mini | 5 |
| google-gemini | google | gemini-2.5-flash, gemini-2.5-pro | 15 |
| openai-compatible | openai | local models (Ollama, vLLM) | 30 |
