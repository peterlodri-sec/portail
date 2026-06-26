#!/usr/bin/env bash
# ralph-loop — version bump decision engine.
# Analyzes commit history since last tag, decides version bump level.
# Posts decision as issue with branch, labels, implementation plan.
# Verbose logging. Never fails CI.

set -euo pipefail
REPO="peterlodri-sec/portail"
BRANCH="ralph/v$(date +%Y%m%d-%H%M)"
LOG_FILE="/tmp/ralph-decision.log"

exec > >(tee -a "$LOG_FILE") 2>&1

echo "🤖 ralph-loop: analyzing commits since last tag..."
echo "   Timestamp: $(date -u)"
echo "   Branch: $BRANCH"
echo ""

# ── Collect metrics ──
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
COMMITS=$(git log "$LAST_TAG..HEAD" --oneline 2>/dev/null | wc -l | tr -d ' ')
FILES_CHANGED=$(git diff "$LAST_TAG..HEAD" --stat 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
BREAKING=$(git log "$LAST_TAG..HEAD" --oneline 2>/dev/null | grep -ci "BREAKING\|breaking change\|!: " || echo 0)
FEATURES=$(git log "$LAST_TAG..HEAD" --oneline 2>/dev/null | grep -ci "^[a-f0-9]* feat:" || echo 0)
FIXES=$(git log "$LAST_TAG..HEAD" --oneline 2>/dev/null | grep -ci "^[a-f0-9]* fix:" || echo 0)

# ── Ralph's decision matrix ──
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
IFS='.' read -r MAJOR MINOR PATCH <<< "${CURRENT_VERSION//[^0-9.]/}"

echo "   Major: $MAJOR  Minor: $MINOR  Patch: $PATCH"
echo "   Commits since $LAST_TAG: $COMMITS"
echo "   Breaking changes: $BREAKING"
echo "   Features: $FEATURES"
echo "   Fixes: $FIXES"

if [ "$BREAKING" -gt 0 ]; then
    NEW_VERSION="$((MAJOR + 1)).0.0"
    LEVEL="MAJOR"
elif [ "$FEATURES" -gt 5 ]; then
    NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
    LEVEL="MINOR"
else
    NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
    LEVEL="PATCH"
fi

echo "   → Ralph's decision: bump to $NEW_VERSION ($LEVEL)"
echo ""

# ── Create implementation plan ──
cat > /tmp/ralph-plan.md << EOF
## 🤖 Ralph-Loop: Version Bump Decision

### Decision
**Bump to \`$NEW_VERSION\`** ($LEVEL bump)

### Metrics
| Metric | Value |
|--------|-------|
| Commits since $LAST_TAG | $COMMITS |
| Breaking changes | $BREAKING |
| Features | $FEATURES |
| Fixes | $FIXES |
| Files changed | $FILES_CHANGED |

### Implementation Plan
1. Update \`Cargo.toml\`: \`version = "$NEW_VERSION"\`
2. Update \`nix/package.nix\`: \`version = "$NEW_VERSION"\`
3. Run \`cargo check && cargo test\`
4. Review generated plan, adjust if needed
5. Merge this PR → auto-tags \`v$NEW_VERSION\`

### Context
- Previous tag: \`$LAST_TAG\`
- Current version: \`$CURRENT_VERSION\`
- Branch: \`$BRANCH\`
- Generated: $(date -u)

> Advisory only. Human review required before merge.
EOF

echo "📝 Ralph decision plan generated"
cat /tmp/ralph-plan.md

# ── Post as GitHub issue (never fail, always exit 0) ──
if command -v gh &>/dev/null; then
    ISSUE_URL=$(gh issue create \
        --repo "$REPO" \
        --title "🤖 Ralph-Loop: Bump to v$NEW_VERSION ($LEVEL)" \
        --body-file /tmp/ralph-plan.md \
        --label "ralph-loop,version-bump,ci-agent" \
        2>/dev/null) || true

    if [ -n "${ISSUE_URL:-}" ]; then
        echo "   Created ralph-loop issue: $ISSUE_URL"

        # Create branch
        git checkout -b "$BRANCH" 2>/dev/null || true

        # Update version files
        if [ "$LEVEL" != "SKIP" ]; then
            sed -i '' "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/g" Cargo.toml 2>/dev/null || true
            sed -i '' "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/g" nix/package.nix 2>/dev/null || true
            git add Cargo.toml nix/package.nix 2>/dev/null || true
            git commit -m "v$NEW_VERSION: ralph-loop auto-bump ($LEVEL)" 2>/dev/null || true
            git push origin "$BRANCH" 2>/dev/null || true

            # Open PR
            gh pr create \
                --repo "$REPO" \
                --head "$BRANCH" \
                --base main \
                --title "🤖 v$NEW_VERSION ($LEVEL bump) — Ralph-Loop" \
                --body "Auto-generated version bump by ralph-loop. See issue for decision context.\n\nCloses ${ISSUE_URL}" \
                2>/dev/null || true
        fi
    fi
fi

echo "✅ ralph-loop complete (always exit 0)"
exit 0
