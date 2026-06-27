# AI Services Flake

**Path:** `nix/ai-services.nix`

Provision and manage AI inference services on macOS.

## System-aware model selection

The flake probes your hardware at runtime and only downloads models that fit:

| Model | Type | Size | Min RAM | Min Disk | Use case |
|-------|------|------|---------|----------|----------|
| `deepseek-coder-v2:16b` | MoE 16B (2.4B active) | ~9GB | 24GB | 12GB | Agentic, 128K ctx, fast |
| `codestral:22b` | Dense 22B | ~14GB | 32GB | 18GB | FIM, refactoring |
| `deepseek-r1:14b` | Dense 14B R1-distill | ~9GB | 24GB | 12GB | Reasoning chains |
| `nomic-embed-text:v1.5` | Embedding 768-dim | ~274MB | 4GB | 1GB | RAG |

**Fail fast:** `ai-check` validates RAM + disk against every model. `ollama-pull-models` skips models that don't fit your hardware.

## Usage

```bash
nix develop .#ai-services

ai-check                       # Validate hardware against models
ollama-serve                   # Start Ollama with Metal
ollama-pull-models             # Only pulls models that fit
ollama-status                  # Check running models
ollama-mcp                     # MCP bridge for agents
tailscale-serve                # Expose via Tailscale
ai-info                        # Full diagnostics
```
