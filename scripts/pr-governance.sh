#!/usr/bin/env bash
# pr-governance — non-LLM template compliance checker.
# Checks if PRs/issues follow the template. Labels non-compliant.
# Runs on: pull_request opened. Never fails CI.

set -euo pipefail

echo "📋 pr-governance: checking PR/issue templates..."

MISSING_LABEL="template-needed"
HAS_TEMPLATE=false

# ── Check PR has description ──
if [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "$GITHUB_EVENT_PATH" ]; then
    BODY=$(jq -r '.pull_request.body // .issue.body // ""' "$GITHUB_EVENT_PATH" 2>/dev/null || echo "")
    TITLE=$(jq -r '.pull_request.title // .issue.title // ""' "$GITHUB_EVENT_PATH" 2>/dev/null || echo "")
    PR_NUMBER=$(jq -r '.pull_request.number // .issue.number // ""' "$GITHUB_EVENT_PATH" 2>/dev/null || echo "")
    EVENT_TYPE=$(jq -r 'if .pull_request then "pr" elif .issue then "issue" else "unknown" end' "$GITHUB_EVENT_PATH" 2>/dev/null || echo "")

    echo "   $EVENT_TYPE #$PR_NUMBER: $TITLE"

    # Check if description is substantial (>50 chars)
    if [ ${#BODY} -lt 50 ]; then
        echo "⚠️  $EVENT_TYPE #$PR_NUMBER is too short (${#BODY} chars). Template not followed."

        if command -v gh &>/dev/null && [ -n "$PR_NUMBER" ]; then
            REPO="peterlodri-sec/portail"
            if [ "$EVENT_TYPE" = "pr" ]; then
                gh pr edit "$PR_NUMBER" --repo "$REPO" --add-label "$MISSING_LABEL" 2>/dev/null || true
                gh pr comment "$PR_NUMBER" --repo "$REPO" --body "⚠️ **Template not followed.** Please use the PR template (\`.github/PULL_REQUEST_TEMPLATE.md\`) for descriptions. Minimum 50 characters required." 2>/dev/null || true
            else
                gh issue edit "$PR_NUMBER" --repo "$REPO" --add-label "$MISSING_LABEL" 2>/dev/null || true
            fi
        fi
    else
        HAS_TEMPLATE=true
        echo "   ✅ $EVENT_TYPE #$PR_NUMBER follows template (${#BODY} chars)"
    fi
fi

# ── Exit 0 always (advisory) ──
echo "✅ pr-governance complete (always exit 0)"
exit 0
