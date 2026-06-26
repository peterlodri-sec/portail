#!/usr/bin/env bash
# loop-state-pm — auto-updates LOOP_STATE.md on every push to main.
# Tracks: version, test count, CI agent count, timestamp.
# Never fails CI. Appends to history.

set -euo pipefail
REPO="peterlodri-sec/portail"

echo "🔄 loop-state-pm: updating LOOP_STATE.md..."

# ── Collect current metrics ──
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
TEST_COUNT=$(cargo test --workspace 2>&1 | grep "passed;" | tail -1 | grep -o "[0-9]* passed" | head -1 | awk '{print $1}' || echo "?")
WARNING_COUNT=$(cargo check --lib --tests 2>&1 | grep -c "warning:" || echo 0)
AGENT_COUNT=$(ls .github/workflows/*.yml 2>/dev/null | wc -l | tr -d ' ')
GIT_HASH=$(git rev-parse --short HEAD)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

echo "   Version: $VERSION"
echo "   Tests: $TEST_COUNT"
echo "   Warnings: $WARNING_COUNT"
echo "   CI agents: $AGENT_COUNT"

# ── Update LOOP_STATE.md ──
cat >> LOOP_STATE.md << EOF

<!-- auto-updated by loop-state-pm on $TIMESTAMP -->
| $TIMESTAMP | $VERSION | $TEST_COUNT tests | $WARNING_COUNT warnings | $AGENT_COUNT agents | $GIT_HASH |
EOF

echo "✅ LOOP_STATE.md updated"

# ── Commit and push if changed ──
if git diff --quiet LOOP_STATE.md; then
    echo "   No changes to LOOP_STATE.md"
else
    git config user.name "loop-state-pm[bot]"
    git config user.email "loop-state-pm[bot]@users.noreply.github.com"
    git add LOOP_STATE.md
    git commit -m "loop-state: $VERSION ($TEST_COUNT tests, $AGENT_COUNT agents) [$GIT_HASH]" || true
    git push origin main || echo "   Push skipped (read-only)"
    echo "   ✅ LOOP_STATE.md auto-updated and pushed"
fi

exit 0
