#!/usr/bin/env bash
# Portail E2E Test Suite — Full user journey: install → serve → CLI → API → agents
# Usage: bash scripts/e2e-test.sh
# Exit: 0 on success, 1 on any failure

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
PASS=0; FAIL=0

pass() { PASS=$((PASS+1)); echo -e "  ${GREEN}✓${NC} $1"; }
fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}✗${NC} $1"; }

# ── Setup ──────────────────────────────────────────────────────────

PORT=18787  # non-standard to avoid conflicts
URL="http://localhost:$PORT"
BINARY="${PORTAIL_BINARY:-portail}"

cleanup() {
    kill $(jobs -p) 2>/dev/null || true
    wait $(jobs -p) 2>/dev/null || true
    rm -f portail.toml /tmp/portail-test.toml
}
trap cleanup EXIT

echo ""
echo "Portail E2E Test Suite"
echo "====================="
echo ""

# ── 1. Install Verification ────────────────────────────────────────

echo "═══ Install Verification ═══"

$BINARY --version > /dev/null 2>&1 && pass "$BINARY --version" || fail "$BINARY --version"

$BINARY doctor > /dev/null 2>&1 && pass "portail doctor" || fail "portail doctor"

# ── 2. Zero-Config Server ──────────────────────────────────────────

echo ""
echo "═══ Server Startup ═══"

cat > /tmp/portail-test.toml << EOF
listen = "127.0.0.1:$PORT"
[rate_limit]
enabled = false
[store]
enabled = false
[telemetry]
enabled = false
EOF

$BINARY serve --config /tmp/portail-test.toml &
sleep 3

curl -sf "$URL/healthz" > /dev/null && pass "GET /healthz → 200" || fail "GET /healthz → 200"
curl -sf "$URL/readyz" > /dev/null && pass "GET /readyz → 200" || fail "GET /readyz → 200"
curl -sf "$URL/dashboard" > /dev/null && pass "GET /dashboard → 200" || fail "GET /dashboard → 200"

# ── 3. API Endpoints ───────────────────────────────────────────────

echo ""
echo "═══ API Endpoints ═══"

HTTP_CODE=$(curl -so /dev/null -w "%{http_code}" -X POST "$URL/a2a/tasks" -H "Content-Type: application/json" -d '{"messages":[{"role":"user","parts":[{"type":"text","text":"hello"}]}]}')
[ "$HTTP_CODE" = "201" ] && pass "POST /a2a/tasks → 201" || fail "POST /a2a/tasks → $HTTP_CODE"

HTTP_CODE=$(curl -so /dev/null -w "%{http_code}" "$URL/a2a/tasks/nonexistent")
[ "$HTTP_CODE" = "404" ] && pass "GET /a2a/tasks/{id} → 404" || fail "GET /a2a/tasks/{id} → $HTTP_CODE"

curl -sf "$URL/.well-known/agent.json" > /dev/null && pass "GET /.well-known/agent.json → 200" || fail "GET /.well-known/agent.json"

curl -sf "$URL/supervisor/status" > /dev/null && pass "GET /supervisor/status → 200" || fail "GET /supervisor/status"

# File cache
HTTP_CODE=$(curl -so /dev/null -w "%{http_code}" -X PUT "$URL/file-cache/e2e-key" -H "Content-Type: application/octet-stream" --data-binary "e2e-data")
[ "$HTTP_CODE" = "201" ] && pass "PUT /file-cache/{key} → 201" || fail "PUT /file-cache/{key} → $HTTP_CODE"

RESULT=$(curl -s "$URL/file-cache/e2e-key")
[ "$RESULT" = "e2e-data" ] && pass "GET /file-cache/{key} matches" || fail "GET /file-cache/{key} mismatch"

curl -so /dev/null -X DELETE "$URL/file-cache/e2e-key" && pass "DELETE /file-cache/{key}" || fail "DELETE /file-cache/{key}"

# Dashboard JSON validation
curl -s "$URL/dashboard" | jq -e '.config_healthy == true' > /dev/null && pass "dashboard.config_healthy == true" || fail "dashboard.config_healthy"
curl -s "$URL/dashboard" | jq -e '.version != null' > /dev/null && pass "dashboard.version exists" || fail "dashboard.version"

# Agent card A2A compliance
curl -s "$URL/.well-known/agent.json" | jq -e '.name == "portail" and .capabilities.streaming == true' > /dev/null && pass "agent card A2A compliant" || fail "agent card A2A"

# ── 4. CLI Commands ────────────────────────────────────────────────

echo ""
echo "═══ CLI Commands ═══"

$BINARY status 2>/dev/null | grep -q "portail" && pass "portail status" || fail "portail status"
$BINARY health 2>/dev/null | grep -q "healthy" && pass "portail health" || fail "portail health"
$BINARY events 2>/dev/null | grep -q "(" && pass "portail events" || fail "portail events"
$BINARY config show --config /tmp/portail-test.toml 2>/dev/null | grep -q "listen" && pass "portail config show" || fail "portail config show"

# ── 5. CI Agents (Advisory, Always Exit 0) ─────────────────────────

echo ""
echo "═══ CI Agents ═══"

$BINARY complexity --ci --ci-report-path /tmp/e2e-complexity.toml 2>/dev/null
[ $? -eq 0 ] && pass "complexity --ci (exit 0)" || fail "complexity --ci (non-zero exit)"
[ -f /tmp/e2e-complexity.toml ] && pass "complexity wrote report" || fail "complexity report missing"

$BINARY spec-verify diff --ci 2>/dev/null
[ $? -eq 0 ] && pass "spec-verify --ci (exit 0)" || fail "spec-verify --ci (non-zero exit)"

bash scripts/rust-chore.sh verify 2>/dev/null
[ $? -eq 0 ] && pass "chore-bot verify (exit 0)" || fail "chore-bot verify"

# ── Cleanup ────────────────────────────────────────────────────────

kill %1 2>/dev/null; wait %1 2>/dev/null || true

# ── Results ────────────────────────────────────────────────────────

echo ""
echo "════════════════════════════════"
echo -e "  ${GREEN}Passed: $PASS${NC}  ${RED}Failed: $FAIL${NC}"
echo "════════════════════════════════"

[ $FAIL -eq 0 ] && exit 0 || exit 1
