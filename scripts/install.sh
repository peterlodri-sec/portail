#!/usr/bin/env bash
# Portail installer - works on Linux, macOS, and NixOS
# Usage: curl -fsSL https://raw.githubusercontent.com/peterlodri-sec/portail/main/scripts/install.sh | bash

set -euo pipefail

REPO="peterlodri-sec/portail"
BINARY="portail"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Detect OS and architecture
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    
    case "$os" in
        Linux*)  os="unknown-linux-gnu" ;;
        Darwin*) os="apple-darwin" ;;
        *)       error "Unsupported OS: $os" ;;
    esac
    
    case "$arch" in
        x86_64*)  arch="x86_64" ;;
        aarch64*) arch="aarch64" ;;
        arm64*)   arch="aarch64" ;;
        *)        error "Unsupported architecture: $arch" ;;
    esac
    
    echo "${arch}-${os}"
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Install via cargo
install_cargo() {
    if command_exists cargo; then
        info "Installing via cargo..."
        cargo install "$BINARY"
        return 0
    fi
    return 1
}

# Install via Nix
install_nix() {
    if command_exists nix; then
        info "Installing via Nix..."
        nix profile install "github:$REPO"
        return 0
    fi
    return 1
}

# Download binary
install_binary() {
    local platform="$1"
    local version="$2"
    local url="https://github.com/$REPO/releases/download/v${version}/${BINARY}-${platform}"
    
    info "Downloading $BINARY v${version} for ${platform}..."
    
    local tmp_dir
    tmp_dir=$(mktemp -d)
    
    if command_exists curl; then
        curl -fsSL "$url" -o "${tmp_dir}/${BINARY}"
    elif command_exists wget; then
        wget -q "$url" -O "${tmp_dir}/${BINARY}"
    else
        error "Neither curl nor wget found. Please install one."
    fi
    
    chmod +x "${tmp_dir}/${BINARY}"
    
    # Try to install to INSTALL_DIR, fallback to ~/.local/bin
    if [ -w "$INSTALL_DIR" ]; then
        mv "${tmp_dir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
        info "Installed to ${INSTALL_DIR}/${BINARY}"
    else
        local user_dir="$HOME/.local/bin"
        mkdir -p "$user_dir"
        mv "${tmp_dir}/${BINARY}" "${user_dir}/${BINARY}"
        info "Installed to ${user_dir}/${BINARY}"
        warn "Add ${user_dir} to your PATH if not already there."
    fi
    
    rm -rf "$tmp_dir"
}

# Get latest version from GitHub
get_latest_version() {
    local url="https://api.github.com/repos/$REPO/releases/latest"
    
    if command_exists curl; then
        curl -fsSL "$url" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/'
    elif command_exists wget; then
        wget -q -O- "$url" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/'
    else
        error "Neither curl nor wget found."
    fi
}

# Main installation
main() {
    info "Installing Portail..."
    
    # Try cargo first (most reliable)
    if install_cargo; then
        info "Installation complete!"
        "$BINARY" --version
        return 0
    fi
    
    # Try Nix
    if install_nix; then
        info "Installation complete!"
        "$BINARY" --version
        return 0
    fi
    
    # Fallback to binary download
    local platform
    platform=$(detect_platform)
    
    local version
    version=$(get_latest_version)
    
    if [ -z "$version" ]; then
        error "Could not determine latest version."
    fi
    
    install_binary "$platform" "$version"
    
    info "Installation complete!"
    "$BINARY" --version
}

main "$@"
