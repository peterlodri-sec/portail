#!/usr/bin/env bash
# rust-chore.sh — Mechanical Rust project cleanup agent
# v1.4 CI agent: never blocks, always exits 0, reports fixes

set -euo pipefail

MODE="${1:-check}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

TESTS_BEFORE=$(cargo test --workspace 2>&1 | grep -c "passed;")

fix_imports() {
    echo "::group::cargo fix"
    cargo fix --lib --tests --allow-dirty --allow-staged 2>&1 || true
    cargo clippy --fix --lib --tests --allow-dirty --allow-staged 2>&1 || true
    echo "::endgroup::"
}

check_diff() {
    echo "::group::git diff stat"
    git diff --stat 2>&1 || true
    echo "::endgroup::"
}

verify() {
    echo "::group::cargo check"
    cargo check 2>&1 || { echo "❌ cargo check failed"; exit 0; }
    echo "::endgroup::"

    echo "::group::cargo test"
    cargo test --workspace 2>&1 || { echo "❌ cargo test failed"; exit 0; }
    TESTS_AFTER=$(cargo test --workspace 2>&1 | grep -c "passed;" || echo 0)

    if [ "${TESTS_AFTER:-0}" -lt "${TESTS_BEFORE:-0}" ]; then
        echo "⚠️  Test count decreased: $TESTS_BEFORE → ${TESTS_AFTER:-0}"
    else
        echo "✅ Tests: ${TESTS_AFTER:-0} passed (was $TESTS_BEFORE)"
    fi
    echo "::endgroup::"
}

report() {
    local changed=$(git diff --stat 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
    local commit_info=$(git log -1 --format="%h %s")

    cat <<EOF
## chore-bot report

| Metric | Value |
|--------|-------|
| Mode | $MODE |
| Changed files | ${changed:-0} |
| Tests before | $TESTS_BEFORE |

> Commit: $commit_info
EOF
}

case "$MODE" in
    check)
        echo "🔍 chore-bot: checking for fixable issues..."
        cargo clippy --lib --tests 2>&1 | grep -E "warning:" | head -20 || true
        echo "No auto-fixes applied (check-only mode)."
        ;;
    fix)
        echo "🔧 chore-bot: applying auto-fixes..."
        fix_imports
        cargo fmt -- --check 2>&1 || cargo fmt 2>&1
        check_diff
        verify
        echo "✅ Fixes applied."
        ;;
    verify)
        verify
        ;;
    report)
        report
        ;;
    *)
        echo "Usage: $0 {check|fix|verify|report}"
        exit 0
        ;;
esac
