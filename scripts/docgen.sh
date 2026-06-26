#!/usr/bin/env bash
# Generate documentation for portail

set -euo pipefail

echo "Generating portail documentation..."

# Build docs
cargo doc --no-deps --document-private-items

# Copy landing page
cp docs/index.html target/doc/index.html

echo "Documentation generated at target/doc/"
echo "Open with: open target/doc/index.html"
