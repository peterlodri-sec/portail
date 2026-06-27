#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────
# Portail — Contributor Setup Script
# v2.1: One-command environment bootstrap
# Usage: bash scripts/contributor-setup.sh
# ──────────────────────────────────────────────────────────
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[setup]${NC} $1"; }
ok()    { echo -e "${GREEN}[  ok]${NC} $1"; }
err()   { echo -e "${RED}[fail]${NC} $1"; }

echo -e "${CYAN}"
echo "╔═══════════════════════════════════════════════╗"
echo "║        Portail — Contributor Setup            ║"
echo "╚═══════════════════════════════════════════════╝"
echo -e "${NC}"

# ── Rust toolchain ────────────────────────────────────
info "Checking Rust toolchain..."
if ! command -v rustc &> /dev/null; then
  info "Installing Rust via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
  ok "Rust installed: $(rustc --version)"
else
  ok "Rust found: $(rustc --version)"
fi

# Ensure minimum MSRV (1.85)
RUST_VERSION=$(rustc --version | grep -oP '\d+\.\d+' | head -1)
if [[ "$(echo "$RUST_VERSION" | cut -d. -f1)" -lt 1 ]] || \
   { [[ "$(echo "$RUST_VERSION" | cut -d. -f1)" -eq 1 ]] && \
     [[ "$(echo "$RUST_VERSION" | cut -d. -f2)" -lt 85 ]]; }; then
  info "Upgrading Rust to 1.85+ (MSRV)..."
  rustup update stable
  ok "Rust upgraded: $(rustc --version)"
fi

# ── Clippy + rustfmt ──────────────────────────────────
info "Installing rustup components..."
rustup component add clippy rustfmt 2>/dev/null || true
ok "Components ready"

# ── Nix (optional) ────────────────────────────────────
if ! command -v nix &> /dev/null; then
  info "Nix not found (optional). Install for full devShell? [y/N]"
  read -r install_nix
  if [[ "$install_nix" =~ ^[Yy]$ ]]; then
    info "Installing Nix..."
    curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install --no-confirm
    . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
    ok "Nix installed"
  fi
else
  ok "Nix found: $(nix --version 2>/dev/null || echo 'present')"
fi

# ── Direnv (optional) ─────────────────────────────────
if ! command -v direnv &> /dev/null; then
  info "Direnv not found (optional, pairs with Nix). Install? [y/N]"
  read -r install_direnv
  if [[ "$install_direnv" =~ ^[Yy]$ ]]; then
    if command -v brew &> /dev/null; then
      brew install direnv
    elif command -v apt &> /dev/null; then
      sudo apt install -y direnv
    else
      info "Install direnv manually: https://direnv.net/docs/installation.html"
    fi
    ok "Direnv installed"
  fi
else
  ok "Direnv found"
fi

# ── Git hooks ─────────────────────────────────────────
info "Installing git pre-commit hooks..."
HOOKS_DIR=".git/hooks"
HOOK_FILE="$HOOKS_DIR/pre-commit"

if [ -f "$HOOK_FILE" ]; then
  info "Pre-commit hook already exists, overwrite? [y/N]"
  read -r overwrite
  if [[ ! "$overwrite" =~ ^[Yy]$ ]]; then
    ok "Skipping pre-commit hook"
  fi
fi

cat > "$HOOK_FILE" << 'HOOKEOF'
#!/usr/bin/env bash
set -euo pipefail
echo "=== Pre-commit: cargo fmt --check ==="
cargo fmt --check || { echo "❌ Formatting — run 'cargo fmt' to fix"; exit 1; }
echo "=== Pre-commit: cargo clippy ==="
cargo clippy --locked --all-targets -- -D warnings || exit 1
echo "=== Pre-commit: cargo check ==="
cargo check --locked || exit 1
echo "✅ Pre-commit passed"
HOOKEOF
chmod +x "$HOOK_FILE"
ok "Pre-commit hook installed at $HOOK_FILE"

# ── Verify build ──────────────────────────────────────
info "Verifying build (cargo check)..."
cargo check --locked 2>&1 | tail -1
ok "Build check passed"

# ── Summary ───────────────────────────────────────────
echo ""
echo -e "${GREEN}╔═══════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║          Setup complete!                       ║${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════════════╝${NC}"
echo ""
echo "  Next steps:"
echo "    cargo test          # Run tests"
echo "    cargo clippy        # Lint check"
echo "    cargo run -- serve  # Start server"
echo "    portail             # TUI dashboard"
echo ""
echo "  See CONTRIBUTING.md for full guide."
