#!/usr/bin/env bash
# trending-packages — daily GitHub trending Rust packages monitor.
# Updates a single issue per week. Links week-to-week. Never fails CI.

set -euo pipefail
REPO="peterlodri-sec/portail"
WEEK_NUM=$(date +%Y-W%V)
ISSUE_TITLE="📊 Trending Rust Packages — $WEEK_NUM"
BODY_FILE="/tmp/trending-report.md"

echo "📊 trending-packages: scanning GitHub trending..."

# Fetch trending Rust repos from GitHub API (last 7 days, sort by stars)
TRENDING=$(curl -sf "https://api.github.com/search/repositories?q=language:rust+created:>=$(date -d'7 days ago' +%Y-%m-%d)&sort=stars&order=desc&per_page=10" 2>/dev/null || echo '{"items":[]}')
ITEMS=$(echo "$TRENDING" | jq -r '.items[] | "| [\(.full_name)](\(.html_url)) | \(.stargazers_count) ⭐ | \(.description // "no description") | \(.language) |"' 2>/dev/null || echo "No trending data available")

cat > "$BODY_FILE" << EOF
## 📊 Trending Rust Packages — $WEEK_NUM

Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

| Package | Stars | Description | Language |
|---------|-------|-------------|----------|
$ITEMS

> **Week link**: \`#\` — previous week's report  
> Updated daily, new issue every Monday. Advisory only.
EOF

# Create or update the weekly issue
if command -v gh &>/dev/null; then
    EXISTING=$(gh issue list --repo "$REPO" --search "Trending Rust Packages - $WEEK_NUM" --state open --json number --jq '.[0].number' 2>/dev/null || echo "")
    if [ -n "$EXISTING" ]; then
        gh issue comment "$EXISTING" --repo "$REPO" --body-file "$BODY_FILE" 2>/dev/null || true
        echo "   Updated weekly issue #$EXISTING"
    else
        NEW=$(gh issue create --repo "$REPO" --title "$ISSUE_TITLE" --body-file "$BODY_FILE" --label "trending,ci-agent" 2>/dev/null)
        echo "   Created new weekly issue: $NEW"
    fi
fi

echo "✅ trending-packages complete"
exit 0
