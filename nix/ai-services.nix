{
  description = "Portail AI Services — Ollama + MLX + Tailscale provisioning for macOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell.url = "github:numtide/devshell";
    devshell.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs @ { self, nixpkgs, flake-parts, devshell, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ devshell.flakeModule ];
      systems = [ "aarch64-darwin" "x86_64-darwin" ];

      perSystem = { pkgs, ... }: let
        # ── Model presets ──────────────────────────────────────────

        # Tier 1: Fast/Agentic — MoE, tiny active params, 128K ctx
        llmFast = {
          id = "deepseek-coder-v2:16b";
          size = "~9GB";                # Q4_K_M quant
          minRam = 24;                  # GB
          minDisk = 12;                 # GB free
          desc = "MoE 16B (2.4B active) — 128K ctx, agentic workflows";
        };

        # Tier 2: Reasoning — dense, FIM, complex logic
        llmReasoning = {
          id = "codestral:22b";
          size = "~14GB";               # Q4_K_M quant
          minRam = 32;
          minDisk = 18;
          desc = "Dense 22B — FIM master, multi-file refactoring";
        };

        # Tier 3: Light/Thinking — chain-of-thought, algorithmic
        llmLight = {
          id = "deepseek-r1:14b";
          size = "~9GB";
          minRam = 24;
          minDisk = 12;
          desc = "14B R1-distill — reasoning chains, algorithmic code";
        };

        # Embedding model for RAG
        llmEmbed = {
          id = "nomic-embed-text:v1.5";
          size = "~274MB";
          minRam = 4;
          minDisk = 1;
          desc = "Embeddings — Nomic v1.5, 768-dim";
        };

        allModels = [ llmFast llmReasoning llmLight llmEmbed ];

        # ── System checks ──────────────────────────────────────────

        genCheckSys = pkgs.writeShellScript "check-system" ''
          set -euo pipefail
          echo ""
          echo "╔══════════════════════════════════════════════════╗"
          echo "║        Portail AI — System Check                ║"
          echo "╚══════════════════════════════════════════════════╝"

          # Unified Memory (macOS)
          RAM_GB=$(sysctl hw.memsize | awk '{print int($2 / 1073741824)}')
          echo "├─ RAM: ${RAM_GB}GB"

          # Disk free on home
          DISK_GB=$(df -h "$HOME" | tail -1 | awk '{print $4}' | sed 's/[A-Za-z]//g')
          DISK_GB=$(printf "%.0f" "$DISK_GB" 2>/dev/null || echo 0)
          echo "├─ Disk free: ${DISK_GB}GB"

          # Apple Silicon check
          CHIP=$(sysctl machdep.cpu.brand_string 2>/dev/null || echo "Apple Silicon")
          echo "├─ Chip: $CHIP"
          IS_METAL=$(system_profiler SPDisplaysDataType 2>/dev/null | grep -c "Metal" || true)
          if [ "$IS_METAL" -gt 0 ]; then echo "├─ Metal GPU: ✅"; else echo "├─ Metal GPU: ❌"; fi

          # LLM validation
          FAIL=0
          echo ""
          echo "├─ Model validation (disk + ram):"

          # Check each model
          ${
            builtins.concatStringsSep "\n" (map (m: ''
              MODEL_ID="${m.id}"
              MIN_RAM=${toString m.minRam}
              MIN_DISK=${toString m.minDisk}
              DESC="${m.desc}"
              RAM_OK=0; DISK_OK=0
              if [ "$RAM_GB" -ge "$MIN_RAM" ]; then RAM_OK=1; fi
              if [ "$DISK_GB" -ge "$MIN_DISK" ]; then DISK_OK=1; fi
              if [ "$RAM_OK" -eq 1 ] && [ "$DISK_OK" -eq 1 ]; then
                echo "├─ ✅ $MODEL_ID — fits ($DESC)"
              else
                echo "├─ ❌ $MODEL_ID — FAILS"
                [ "$RAM_OK" -eq 0 ] && echo "│  └─ Needs ${MIN_RAM}GB RAM (have ${RAM_GB})"
                [ "$DISK_OK" -eq 0 ] && echo "│  └─ Needs ${MIN_DISK}GB disk (have ${DISK_GB})"
                FAIL=1
              fi
            '') allModels)
          }

          echo "└─ $([ "$FAIL" -eq 0 ] && echo '✅ All checks pass' || echo '❌ Some models will not fit — skipping those')"
          exit "$FAIL"
        '';

        # ── Gen pull command (with system-aware filtering) ─────────
        genPull = pkgs.writeShellScript "ollama-pull-models" ''
          set -euo pipefail

          RAM_GB=$(sysctl hw.memsize | awk '{print int($2 / 1073741824)}')
          DISK_GB=$(df -h "$HOME" | tail -1 | awk '{print $4}' | sed 's/[A-Za-z]//g')
          DISK_GB=$(printf "%.0f" "$DISK_GB" 2>/dev/null || echo 0)

          ${pkgs.ollama}/bin/ollama serve &
          OLLAMA_PID=$!
          sleep 3

          PULLED=0
          SKIPPED=0

          ${
            builtins.concatStringsSep "\n" (map (m: ''
              if [ "$RAM_GB" -ge ${toString m.minRam} ] && [ "$DISK_GB" -ge ${toString m.minDisk} ]; then
                echo "Pulling ${m.id} (${m.size})..."
                ${pkgs.ollama}/bin/ollama pull "${m.id}"
                PULLED=$((PULLED + 1))
              else
                echo "Skipping ${m.id} — needs ${toString m.minRam}GB RAM / ${toString m.minDisk}GB disk"
                SKIPPED=$((SKIPPED + 1))
              fi
            '') allModels)
          }

          kill "$OLLAMA_PID" 2>/dev/null || true
          wait "$OLLAMA_PID" 2>/dev/null || true
          echo ""
          echo "Done: $PULLED pulled, $SKIPPED skipped (system constraints)"
        '';

      in {
        devshells = {
          default = {
            name = "portail-ai";
            description = "Portail AI services (Ollama + Tailscale + models)";

            commands = [
              {
                name = "ollama-serve";
                help = "Start Ollama server with Metal acceleration";
                command = ''
                  export OLLAMA_HOST="0.0.0.0:11434"
                  export OLLAMA_KEEP_ALIVE="5m"
                  export OLLAMA_NUM_PARALLEL="4"
                  export OLLAMA_MAX_LOADED_MODELS="3"
                  export OLLAMA_FLASH_ATTENTION="1"
                  exec ${pkgs.ollama}/bin/ollama serve
                '';
              }
              {
                name = "ollama-pull-models";
                help = "Download models that fit your system (checks RAM + disk)";
                command = ''
                  exec ${genPull}
                '';
              }
              {
                name = "ollama-status";
                help = "Check Ollama and cached models";
                command = ''
                  curl -sf http://localhost:11434/api/tags 2>/dev/null \
                    | python3 -m json.tool 2>/dev/null \
                    | grep -E "name|parameter_size|quant" \
                    || echo "Ollama not running"
                  echo "---"
                  du -sh ~/.ollama/models 2>/dev/null || echo "No models cached"
                '';
              }
              {
                name = "ollama-mcp";
                help = "Start Ollama MCP bridge";
                command = ''
                  if ! command -v uvx &>/dev/null; then
                    curl -LsSf https://astral.sh/uv/install.sh | sh
                  fi
                  exec uvx ollama-mcp --port 11435
                '';
              }
              {
                name = "ai-check";
                help = "Check if your system can run the configured models";
                command = ''
                  exec ${genCheckSys}
                '';
              }
              {
                name = "tailscale-serve";
                help = "Expose portail via Tailscale";
                command = ''
                  if ! command -v tailscale &>/dev/null; then
                    echo "Tailscale not found — install via: nix shell nixpkgs#tailscale"
                    exit 1
                  fi
                  sudo tailscale serve --bg --https=443 localhost:8788
                '';
              }
              {
                name = "models-disk";
                help = "Show model cache disk usage";
                command = ''
                  echo "=== Ollama ==="
                  du -sh ~/.ollama/models 2>/dev/null || echo "(none)"
                  echo "=== HuggingFace ==="
                  du -sh ~/.cache/huggingface 2>/dev/null || echo "(none)"
                '';
              }
              {
                name = "ai-info";
                help = "Full AI hardware/software diagnostics";
                command = ''
                  echo "=== System ==="
                  sw_vers 2>/dev/null | head -2
                  system_profiler SPHardwareDataType 2>/dev/null | grep -E "Memory|Chip" | head -2
                  echo "=== Ollama ==="
                  which ollama 2>/dev/null && ollama --version 2>/dev/null || echo "not installed"
                  echo "=== MLX ==="
                  python3 -c "import mlx; print(f'v{mlx.__version__}')" 2>/dev/null || echo "not installed"
                  echo "=== PyTorch Metal ==="
                  python3 -c "import torch; print(f'MPS: {torch.backends.mps.is_available()}')" 2>/dev/null || echo "not installed"
                '';
              }
            ];

            packages = with pkgs; [ ollama curl python3 jq ];

            env = [
              { name = "OLLAMA_HOST"; value = "0.0.0.0:11434"; }
              { name = "OLLAMA_KEEP_ALIVE"; value = "5m"; }
              { name = "OLLAMA_NUM_PARALLEL"; value = "4"; }
              { name = "OLLAMA_FLASH_ATTENTION"; value = "1"; }
            ];
          };
        };
      };
    };
}
