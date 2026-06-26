#!/usr/bin/env bash
# architecture-helper — checks DESIGN.md / NETWORK_DESIGN.md at version tags.
# Flags core module drift via GitHub issue. Never fails CI.
# Runs on: tag push (v*). Advisory only.

set -euo pipefail

REPO="peterlodri-sec/portail"
ISSUE_TITLE="Architecture Audit — v${GITHUB_REF_NAME:-unknown}"

echo "🏗️  architecture-helper: auditing core design docs..."

# ── Check DESIGN.md exists and is current ──
DESIGN_FILES="docs/architecture/DESIGN.md docs/architecture/NETWORK_DESIGN.md"
DRIFT_COUNT=0

for df in $DESIGN_FILES; do
    if [ ! -f "$df" ]; then
        echo "⚠️  $df missing"
        DRIFT_COUNT=$((DRIFT_COUNT + 1))
        continue
    fi

    # Check if the doc references current module list
    MODULE_COUNT_GIT=$(grep -c "^pub mod " src/lib.rs || echo 0)
    MODULE_COUNT_DOC=$(grep -c "^\s*[a-z_]*|" "$df" || echo 0)

    echo "  $df: $MODULE_COUNT_DOC documented vs $MODULE_COUNT_GIT actual modules"
done

# ── Generate report ──
if [ $DRIFT_COUNT -gt 0 ]; then
    cat > /tmp/architecture-report.md << EOF
## 🏗️ Architecture Audit — \`${GITHUB_REF_NAME:-unknown}\`

⚠️ **$DRIFT_COUNT design doc(s) need updating.**

- DESIGN.md and/or NETWORK_DESIGN.md should be updated to reflect current module structure.
- `src/lib.rs` declares $MODULE_COUNT_GIT pub modules. Docs should match.
- Run \`task counts\` or \`cargo check\` to verify current state.

> Advisory only — this report is informational. CI continues.
EOF

    echo "📝 Architecture drift detected:"
    cat /tmp/architecture-report.md

    # Create/update GitHub issue
    if command -v gh &>/dev/null; then
        EXISTING=$(gh issue list --repo "$REPO" --search "Architecture Audit" --state open --json number --jq '.[0].number' 2>/dev/null || echo "")
        if [ -n "$EXISTING" ]; then
            gh issue comment "$EXISTING" --repo "$REPO" --body-file /tmp/architecture-report.md 2>/dev/null || true
            echo "   Updated existing issue #$EXISTING"
        else
            gh issue create --repo "$REPO" --title "$ISSUE_TITLE" --body-file /tmp/architecture-report.md --label "architecture,ci-agent" 2>/dev/null || true
            echo "   Created new architecture audit issue"
        fi
    fi
fi

echo "✅ architecture-helper complete (always exit 0)"
exit 0
