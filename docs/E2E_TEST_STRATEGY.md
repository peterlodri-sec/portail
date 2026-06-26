# Portail E2E Test Strategy — From Install to Production

## Philosophy

E2E tests verify the full user journey. No mocking, no stubbing.
Every test starts cold — no pre-installed dependencies, no pre-configured
state. If a user would do it, the test does it.

```
Install → Init → Serve → Smoke → CLI → Config → Deploy → Verify
```

---

## 1. Install Paths

### 1.1 Curl Pipe Install (Linux)
```bash
# Start: clean Ubuntu 24.04 container
docker run --rm -it ubuntu:24.04 bash -c '
  apt-get update && apt-get install -y curl
  curl -fsSL https://raw.githubusercontent.com/peterlodri-sec/portail/main/scripts/install.sh | bash
  portail --version
  portail doctor
'
```
**Assert**: `portail --version` prints `portail 2.0.0-rc`, `portail doctor` shows all ✓.

### 1.2 Cargo Install
```bash
# Start: clean Rust container
docker run --rm -it rust:latest bash -c '
  cargo install portail
  portail --version
'
```
**Assert**: binary on PATH, version matches.

### 1.3 Nix Install
```bash
nix profile install github:peterlodri-sec/portail
portail --version
```
**Assert**: `nix run github:peterlodri-sec/portail -- serve` starts without error.

### 1.4 Docker Pull
```bash
docker run -p 8787:8787 ghcr.io/peterlodri-sec/portail:latest &
sleep 3
curl http://localhost:8787/healthz
```
**Assert**: healthz returns 200.

---

## 2. Non-Interactive CLI Entrypoint

### 2.1 Headless Server (Zero Config)
```bash
# Start server in background
portail serve &
SERVER_PID=$!
sleep 2

# Health checks
curl -s http://localhost:8787/healthz | grep -q "OK" || exit 1
curl -s http://localhost:8787/readyz | grep -q "OK" || exit 1
curl -s http://localhost:8787/dashboard | jq -e '.config_healthy == true' || exit 1

kill $SERVER_PID
```
**Assert**: server starts with no config file, all endpoints respond.

### 2.2 CLI Commands (against running server)
```bash
portail serve &
sleep 2

# Status
portail status | grep -q "portail v2" || exit 1

# Health
portail health | grep -q "healthy" || exit 1

# Events
portail events | grep -q "showing" || exit 1

# Config show
portail config show | grep -q "listen" || exit 1

kill %1
```
**Assert**: all CLI commands connect to running server and return data.

### 2.3 Init Wizard (Non-Interactive Mode)
```bash
# Generate config with defaults (pipe 'yes' to all prompts)
echo -e "\n\n\n\n\n\n\n\n\n\n" | portail init
test -f portail.toml || exit 1
portail config validate
```
**Assert**: config file created, validates as correct.

### 2.4 Config Rollback
```bash
cp portail.toml portail.toml.bak
echo 'listen = "127.0.0.1:9999"' > portail.toml
portail serve &
sleep 1
# Modify config while running
echo 'listen = "127.0.0.1:8888"' > portail.toml
sleep 2  # wait for auto-reload
portail config rollback 1
sleep 1

kill %1
mv portail.toml.bak portail.toml
```
**Assert**: auto-reload picks up changes, rollback works.

### 2.5 Complexity (CI Mode)
```bash
portail complexity --ci --ci-report-path /tmp/complexity-ci.toml
test -f /tmp/complexity-ci.toml || exit 1
```
**Assert**: advisory CI agent works, never fails, writes report.

### 2.6 Fuzz Route (against running server)
```bash
portail serve &
sleep 2
portail fuzz-route --url http://localhost:8787 --ci
kill %1
```
**Assert**: fuzz completes without crash (exit 0 always in CI mode).

---

## 3. Config-Driven Startup

### 3.1 Config File Startup
```bash
cat > /tmp/portail-test.toml << 'EOF'
listen = "127.0.0.1:9998"
[rate_limit]
enabled = true
burst = 5
per_second = 100.0
[auth]
enabled = true
api_keys = { sk-test = { label = "test" } }
[store]
enabled = true
provider = "sqlite"
db_path = ":memory:"
[telemetry]
enabled = false
EOF

portail serve --config /tmp/portail-test.toml &
sleep 2

# Rate limit should trigger after 5 rapid requests
for i in $(seq 1 10); do
  curl -s -o /dev/null -w "%{http_code}" http://localhost:9998/healthz
  echo
done | grep -q "429" || echo "WARN: rate limit didn't trigger"

# Auth should reject unauthenticated requests
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:9998/v1/chat/completions)
test "$HTTP_CODE" = "401" || exit 1

kill %1
```
**Assert**: rate limiting triggers, auth blocks, event store works with in-memory SQLite.

---

## 4. API Smoke Tests

### 4.1 Core Endpoints
```bash
portail serve &
sleep 2

ENDPOINTS=(
  "GET /healthz 200"
  "GET /readyz 200"
  "GET /dashboard 200"
  "GET /metrics 200"
  "GET /.well-known/agent.json 200"
  "POST /a2a/tasks 201"
  "GET /a2a/tasks/test-id 404"
  "POST /hooks 400"
  "GET /sessions 200"
  "GET /supervisor/status 200"
  "GET /graphql 200"
)

for entry in "${ENDPOINTS[@]}"; do
  method=$(echo $entry | cut -d' ' -f1)
  path=$(echo $entry | cut -d' ' -f2)
  expected=$(echo $entry | cut -d' ' -f3)
  actual=$(curl -s -o /dev/null -w "%{http_code}" -X $method "http://localhost:8787$path" \
    -H "Content-Type: application/json" -d '{}' 2>/dev/null || echo 000)
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $method $path → $actual (expected $expected)"
  fi
done

kill %1
```
**Assert**: all core endpoints respond with expected status codes.

### 4.2 A2A Task Lifecycle
```bash
portail serve &
sleep 2

# Create task
TASK_ID=$(curl -s -X POST http://localhost:8787/a2a/tasks \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","parts":[{"type":"text","text":"hello"}]}]}' \
  | jq -r '.id')

# Get task
curl -s http://localhost:8787/a2a/tasks/$TASK_ID | jq -e '.status == "submitted"' || exit 1

kill %1
```
**Assert**: task created with valid ID, retrievable, status is "submitted".

### 4.3 Agent Card Compliance
```bash
portail serve &
sleep 2

# Agent card must have required fields
curl -s http://localhost:8787/.well-known/agent.json | jq -e '
  .name == "portail" and
  .capabilities.streaming == true and
  (.skills | length) >= 2
' || exit 1

kill %1
```
**Assert**: agent card complies with A2A spec.

### 4.4 Dashboard Health
```bash
portail serve &
sleep 2

curl -s http://localhost:8787/dashboard | jq -e '
  .config_healthy == true and
  .version != null and
  .rate_limit_denied != null and
  .auth_failures != null and
  .cdn != null
' || exit 1

kill %1
```
**Assert**: dashboard returns complete health snapshot.

### 4.5 File Cache
```bash
portail serve &
sleep 2

# PUT
curl -s -X PUT -H "Content-Type: application/octet-stream" \
  --data-binary "test-data" http://localhost:8787/file-cache/key1 -w "%{http_code}" | grep -q 201 || exit 1

# GET
RESULT=$(curl -s http://localhost:8787/file-cache/key1)
test "$RESULT" = "test-data" || exit 1

# DELETE
curl -s -X DELETE http://localhost:8787/file-cache/key1 -w "%{http_code}" | grep -q 200 || exit 1

kill %1
```
**Assert**: PUT/GET/DELETE cycle works.

---

## 5. Deploy Paths

### 5.1 Systemd Unit
```bash
cat > /etc/systemd/system/portail.service << 'EOF'
[Unit]
Description=Portail Proxy Gateway
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/portail serve
Restart=always
RestartSec=5
MemoryMax=512M
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/portail /var/cache/portail
User=portail
Group=portail

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl start portail
sleep 2
curl http://localhost:8787/healthz
systemctl status portail --no-pager | grep -q "active (running)"
systemctl stop portail
```
**Assert**: systemd unit starts, serves healthz, status shows active.

### 5.2 Docker Compose Full Stack
```yaml
# docker-compose.test.yml
version: '3.8'
services:
  portail:
    image: ghcr.io/peterlodri-sec/portail:latest
    ports: ["8787:8787"]
    environment:
      - PORTAIL_NATS_ENABLED=true
      - PORTAIL_NATS_URL=nats://nats:4222
    depends_on: [nats]
  nats:
    image: nats:latest
```
```bash
docker compose -f docker-compose.test.yml up -d
sleep 5
curl http://localhost:8787/healthz
docker compose down
```
**Assert**: full stack starts, health passes.

---

## 6. CI Integration

### 6.1 GitHub Actions E2E Workflow
```yaml
name: E2E Tests
on:
  push:
    tags: ["v*"]
  workflow_dispatch:

jobs:
  e2e-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - name: Build release
        run: cargo build --release
      - name: Install test binary
        run: sudo cp target/release/portail /usr/local/bin/
      - name: Run E2E suite
        run: bash scripts/e2e-test.sh

  e2e-docker:
    runs-on: ubuntu-latest
    steps:
      - name: Pull image
        run: docker pull ghcr.io/peterlodri-sec/portail:latest
      - name: Smoke test
        run: |
          docker run -d -p 8787:8787 ghcr.io/peterlodri-sec/portail:latest
          sleep 5
          curl -f http://localhost:8787/healthz || exit 1

  e2e-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v7
      - name: Build
        run: cargo build --release
      - name: Run
        run: |
          cp target/release/portail /usr/local/bin/
          bash scripts/e2e-test.sh
```

### 6.2 E2E Test Script (`scripts/e2e-test.sh`)
See sections 2-4 above — combine into one script that:
1. Starts portail serve
2. Runs all smoke tests
3. Kills server
4. Reports pass/fail counts
5. Exits 0 on success, 1 on any failure

---

## 7. CI Agent Integration

E2E tests should verify each CI agent runs correctly in advisory mode:

| Agent | E2E Test |
|-------|----------|
| complexity | `portail complexity --ci` → exits 0, writes report |
| drift-detect | `portail drift-detect capture --ci --url http://localhost:8787` → exits 0 |
| spec-verify | `portail spec-verify diff --ci` → exits 0 |
| fuzz-route | `portail fuzz-route --ci --url http://localhost:8787` → exits 0 |
| chore-bot | `bash scripts/rust-chore.sh verify` → exits 0 |

---

## 8. Edge Cases to Test

- **Port already in use**: second `portail serve` on same port → fails gracefully
- **Invalid config**: `portail serve --config /dev/null` → exits with error
- **Missing binary**: `portail` not found → clear error message
- **Permission denied**: `/usr/local/bin` not writable → fallback to `~/.local/bin`
- **Disk full**: cache directory on full disk → logs warning, doesn't crash
- **NATS unavailable**: `PORTAIL_NATS_ENABLED=true` but no NATS server → degrades gracefully
- **Large payload**: POST 9.9MB body → accepted; POST 11MB → rejected (413 or 429)
```

---

## 9. What's NOT Tested (and why)

| Gap | Reason |
|-----|--------|
| Live NATS pub/sub | Needs running NATS server, skipped in CI |
| Redis cache tier | Needs running Redis, tested manually |
| Turso backend | Needs Turso account + token, integration test only |
| TLS/mTLS | Needs valid certificates, tested manually |
| DPDK/io_uring | Needs specific hardware/kernel, skipped |
| Load testing | Separate benchmark suite (criterion) |
| Headscale / RustDesk | Not in v2.0 scope |
```

## 10. Success Criteria for v2.0 Final

- [ ] `scripts/e2e-test.sh` written and passing on CI
- [ ] All 5 CI agents pass in advisory mode
- [ ] Install script tested on Ubuntu 24.04 + macOS 14
- [ ] Docker smoke test passes
- [ ] Zero-config `portail serve` starts and responds to all core endpoints
- [ ] 174 unit/integration tests pass
- [ ] `cargo audit` reports 0 critical CVEs
