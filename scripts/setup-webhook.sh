#!/usr/bin/env bash
# Portail Webhook Setup - One-time secure configuration
# Usage: ./scripts/setup-webhook.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
error() { echo -e "${RED}[✗]${NC} $1"; exit 1; }

# Configuration
REPO="peterlodri-sec/portail"
SECRET_FILE="$HOME/.config/portail/webhook-secret"
ENV_FILE="$HOME/.config/portail/.env"
SYSTEMD_FILE="/etc/systemd/system/portail.service"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║          Portail Webhook Setup (One-time)                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Step 1: Generate secure secret
info "Generating secure webhook secret..."
SECRET=$(openssl rand -hex 32)
info "Secret generated: ${SECRET:0:8}...${SECRET: -8}"

# Step 2: Store secret securely
info "Storing secret in $SECRET_FILE..."
mkdir -p "$HOME/.config/portail"
echo -n "$SECRET" > "$SECRET_FILE"
chmod 600 "$SECRET_FILE"
info "Secret stored with 600 permissions"

# Step 3: Create .env file
info "Creating environment file..."
cat > "$ENV_FILE" << EOF
# Portail Environment Variables
# Generated: $(date -u '+%Y-%m-%d %H:%M:%S UTC')

PORTAIL_WEBHOOK_SECRET=$SECRET
PORTAIL_LISTEN=0.0.0.0:8787
RUST_LOG=info
EOF
chmod 600 "$ENV_FILE"
info "Environment file created"

# Step 4: Configure GitHub webhook
info "Configuring GitHub webhook..."

# Check if gh CLI is available
if ! command -v gh &> /dev/null; then
    warn "gh CLI not found. Install it to auto-configure GitHub webhook."
    warn "  brew install gh"
    warn "  gh auth login"
else
    # Get the webhook URL
    WEBHOOK_URL="https://portail.vaked.dev/ci/webhook"
    
    # Create webhook
    gh api repos/$REPO/hooks \
        --method POST \
        --field name="web" \
        --field active=true \
        --field config[url]="$WEBHOOK_URL" \
        --field config[content_type]="json" \
        --field config[secret]="$SECRET" \
        --field config[insecure_ssl]="0" \
        --field events[]="workflow_run" \
        --field events[]="workflow_job" \
        --field events[]="check_run" 2>/dev/null && \
        info "GitHub webhook configured" || \
        warn "Could not configure webhook automatically (may already exist)"
fi

# Step 5: Create systemd service (if on Linux)
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    info "Creating systemd service..."
    
    # Get the portail binary path
    PORTAIL_BIN=$(which portail 2>/dev/null || echo "/usr/local/bin/portail")
    
    cat > /tmp/portail.service << EOF
[Unit]
Description=Portail - Unified Proxy/Gateway
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$HOME
ExecStart=$PORTAIL_BIN serve
Environment=PORTAIL_WEBHOOK_SECRET=$SECRET
Environment=PORTAIL_LISTEN=0.0.0.0:8787
Environment=RUST_LOG=info
Restart=always
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    info "Systemd service template created at /tmp/portail.service"
    warn "To install: sudo cp /tmp/portail.service /etc/systemd/system/"
    warn "Then: sudo systemctl daemon-reload && sudo systemctl enable portail"
fi

# Step 6: Print summary
echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║                    Setup Complete!                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Secret stored in: $SECRET_FILE"
echo "Environment file: $ENV_FILE"
echo ""
echo "To start portail:"
echo "  source $ENV_FILE"
echo "  portail serve"
echo ""
echo "Or with systemd:"
echo "  sudo systemctl start portail"
echo ""
echo "GitHub webhook URL: https://portail.vaked.dev/ci/webhook"
echo ""
echo "Verify webhook:"
echo "  curl -X POST https://portail.vaked.dev/ci/webhook \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -H 'X-GitHub-Event: ping' \\"
echo "    -H 'X-Hub-Signature-256: sha256=\$(echo -n \"{}\" | openssl dgst -sha256 -hmac \"$SECRET\" | awk '{print \$2}')' \\"
echo "    -d '{}'"
echo ""
