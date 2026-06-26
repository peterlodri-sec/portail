# Portail E2E Scenarios — Expected Flows & Outcomes

> For every scenario: define the target state, every step, and the expected
> outcome. These are the acceptance criteria for v2.0 production-ready status.

---

## Scenario 1: MacBook Developer — Self-Signed Cert + Local AI

### Context

A developer on macOS (aarch64) wants to run Portail locally to proxy
requests to Ollama running on the same machine. They use a self-signed
certificate because they don't have a domain.

### Target State

```
┌─────────────────────────────────────────────────────┐
│                    macOS (aarch64)                   │
│                                                     │
│  Terminal 1:  ollama serve                          │
│  Terminal 2:  portail serve --config portail.toml   │
│  Terminal 3:  curl https://localhost:8787/healthz   │
│                                                     │
│  Ollama ←── portail (HTTPS, self-signed) ←── curl   │
│                                                     │
│  TLS:    Self-signed cert (portail.crt + portail.key)│
│  Cache:  Moka in-memory, no Redis                    │
│  Store:  SQLite in-memory (:memory:)                 │
│  NATS:   Disabled                                    │
└─────────────────────────────────────────────────────┘
```

### Flow

```bash
# Step 1: Install
curl -fsSL https://raw.githubusercontent.com/peterlodri-sec/portail/main/scripts/install.sh | bash
# Expected: binary at /usr/local/bin/portail, version prints

# Step 2: Generate self-signed cert
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout portail.key -out portail.crt \
  -days 365 -subj "/CN=localhost"
# Expected: portail.key + portail.crt created

# Step 3: Init config
portail init
# Interactive: listen=0.0.0.0:8787, TLS cert+key paths, AI gateway→http://localhost:11434
# Expected: portail.toml created

# Step 4: Start server
cat > portail.toml << 'EOF'
listen = "0.0.0.0:8787"
tls_cert = "portail.crt"
tls_key = "portail.key"

[ai_gateway]
enabled = true
upstream = "http://localhost:11434"

[rate_limit]
enabled = true
burst = 30
per_second = 10.0

[store]
enabled = true
provider = "sqlite"
db_path = ":memory:"
retention_days = 0

[telemetry]
enabled = false
EOF

portail serve &
sleep 2

# Step 5: Verify health
curl -k https://localhost:8787/healthz
# Expected: 200 OK with "OK" body

# Step 6: Proxy AI request
curl -k https://localhost:8787/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3","messages":[{"role":"user","content":"hello"}]}'
# Expected: 200 OK with streaming or JSON response from Ollama

# Step 7: Verify dashboard
curl -k https://localhost:8787/dashboard | jq .
# Expected: { config_healthy: true, rate_limit_denied: 0, auth_failures: 0, cdn: {...} }

# Step 8: CLI commands work against running server
portail status
# Expected: prints version, listen addr, feature flags, "server: running"

portail health
# Expected: "server is healthy"

portail events
# Expected: shows recent proxy events
```

### Expected Outcomes

| Check | What to verify |
|-------|---------------|
| Install works | Binary on PATH, `--version` prints |
| Self-signed cert | `openssl` generates valid cert+key pair |
| Init wizard | Config file created with correct defaults |
| Server starts | Binds to 0.0.0.0:8787 with TLS |
| Health endpoint | 200 OK, both with -k (accept self-signed) |
| AI proxy | Forwards to Ollama, returns valid response |
| Dashboard | JSON with config_healthy=true |
| CLI commands | All connect to running server, return data |
| Rate limiting | Works with burst=30 (test with rapid requests) |

---

## Scenario 2: Linux VPS — Docker + Let's Encrypt + Redis

### Context

A startup deploys Portail on a $20/month VPS (Ubuntu 24.04, x86_64).
They have a domain (api.example.com), want automatic TLS from Let's
Encrypt, and use Redis for distributed caching.

### Target State

```
┌──────────────────────────────────────────────────────────┐
│           VPS (Ubuntu 24.04, x86_64, 2 vCPU, 4GB)        │
│                                                          │
│  Docker Compose:                                         │
│  ┌────────────┐  ┌──────────┐  ┌──────────┐             │
│  │  portail   │  │  redis   │  │  nats    │             │
│  │  :8787     │  │  :6379   │  │  :4222   │             │
│  └─────┬──────┘  └────┬─────┘  └────┬─────┘             │
│        │               │              │                   │
│  ┌─────┴───────────────┴──────────────┴─────┐            │
│  │            Docker network                │            │
│  └──────────────────────────────────────────┘            │
│                                                          │
│  Internet ←── Nginx (443) → portail (8787) → Upstream    │
│                                                          │
│  TLS:    Let's Encrypt (via Certbot or Caddy sidecar)    │
│  Cache:  Redis (network-wide, TTL 1h)                    │
│  Store:  SQLite on disk (/var/lib/portail/events.db)     │
│  NATS:   Enabled (cache invalidation + event bridge)     │
│  Auth:   API key required for all /v1/* routes           │
└──────────────────────────────────────────────────────────┘
```

### Flow

```bash
# Step 1: Pull image
docker pull ghcr.io/peterlodri-sec/portail:latest
# Expected: image pulled, tag shows latest

# Step 2: Create docker-compose.yml
cat > docker-compose.yml << 'EOF'
version: '3.8'
services:
  portail:
    image: ghcr.io/peterlodri-sec/portail:latest
    ports: ["8787:8787"]
    volumes:
      - ./portail.toml:/etc/portail/portail.toml:ro
      - /var/lib/portail:/var/lib/portail
    environment:
      - PORTAIL_NATS_ENABLED=true
      - PORTAIL_NATS_URL=nats://nats:4222
    depends_on: [redis, nats]

  redis:
    image: redis:7-alpine
    ports: ["6379:6379"]

  nats:
    image: nats:2-alpine
    ports: ["4222:4222"]
EOF

cat > portail.toml << 'EOF'
listen = "0.0.0.0:8787"

[ai_gateway]
enabled = true
upstream = "https://api.openai.com"

[redis]
enabled = true
url = "redis://redis:6379"
max_memory_mb = 2048

[nats]
enabled = true

[rate_limit]
enabled = true
burst = 30
per_second = 10.0

[auth]
enabled = true
api_keys = { sk-prod-abc123 = { label = "production" } }

[store]
enabled = true
provider = "nats"
db_path = "/var/lib/portail/events.db"
retention_days = 30

[telemetry]
enabled = true
endpoint = "http://jaeger:4317"
sampling_ratio = 0.1
EOF

# Step 3: Start stack
docker compose up -d
sleep 5

# Step 4: Verify services
docker compose ps
# Expected: all 3 services "Up" (healthy)

# Step 5: Health checks
curl http://localhost:8787/healthz
# Expected: 200 OK

curl http://localhost:8787/readyz
# Expected: 200 OK (Redis + NATS connected)

curl http://localhost:8787/dashboard | jq .
# Expected: { config_healthy: true, cdn: { hits: 0, misses: 0 } }

# Step 6: Auth required
curl http://localhost:8787/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}'
# Expected: 401 Unauthorized

# Step 7: Auth works
curl http://localhost:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-prod-abc123" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}'
# Expected: 200 OK (or 502 if API key is test key)

# Step 8: Rate limiting
for i in $(seq 1 50); do
  curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8787/healthz
done
# Expected: some responses are 429 (rate limited after ~30 bursts)

# Step 9: Redis cache working
# Send same request twice, second should be faster (cache hit)
curl -w "\n%{time_total}s\n" http://localhost:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-prod-abc123" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"say exactly hello"}]}'

# Step 10: Cleanup
docker compose down
```

### Expected Outcomes

| Check | What to verify |
|-------|---------------|
| Docker pull | Image available on ghcr.io |
| Stack starts | 3 services healthy, no crash loops |
| Redis connected | `/readyz` returns 200 (checks dependency health) |
| NATS connected | Event bridge connects, publishes events |
| Auth blocks | 401 on unauthenticated /v1/* requests |
| Auth allows | 200 with valid API key |
| Rate limiting | 429 responses after burst exceeded |
| Redis cache | Cache hit improves latency on repeated requests |
| Persistent store | Events survive restart (docker compose down/up) |

---

## Scenario 3: NixOS Home Lab — Multi-Node + NATS Replication

### Context

A homelab enthusiast runs Portail on two NixOS machines (x86_64 + aarch64).
They use NATS for event replication and cache invalidation between nodes.
No external dependencies — everything is self-hosted.

### Target State

```
┌─────────────────────┐     NATS     ┌─────────────────────┐
│    Node 1 (x86_64)  │◄────────────►│   Node 2 (aarch64)  │
│                     │              │                     │
│  portail :8787      │  portail.    │  portail :8787      │
│  nats-server :4222  │  store.      │  nats-server :4222  │
│                     │  events      │                     │
│  events.db (local)  │              │  events.db (local)  │
└─────────────────────┘              └─────────────────────┘
                                              │
                                        Nginx LB (:443)
                                              │
                                        Internet
```

### Flow

```bash
# Step 1: Install on both nodes
# Node 1 (x86_64):
nix profile install github:peterlodri-sec/portail

# Node 2 (aarch64):
nix profile install github:peterlodri-sec/portail

# Step 2: Start NATS on Node 1
nix run nixpkgs#nats-server -- -p 4222 -m 8222 &
# Expected: NATS running, management UI at :8222

# Step 3: Configure both nodes
cat > /etc/portail/portail.toml << 'EOF'
listen = "0.0.0.0:8787"

[rate_limit]
enabled = true
burst = 30

[auth]
enabled = true
api_keys = { sk-node = { label = "node" } }

[store]
enabled = true
provider = "nats"
db_path = "/var/lib/portail/events.db"
retention_days = 30

[cdn]
enabled = true
origin = "http://127.0.0.1:9000"
cache_dir = "/var/cache/portail"
cache_size = "10g"
nats_url = "nats://localhost:4222"

[telemetry]
enabled = false
EOF

# Step 4: Start portail on both nodes
systemctl enable --now portail
# Expected: service active (running), ports bound

# Step 5: Create event on Node 1
curl -X POST http://node1:8787/events \
  -H "Authorization: Bearer sk-node" \
  -H "Content-Type: application/json" \
  -d '{"agent_id":"test","event_type":"deploy","severity":"info","metadata":{"version":"2.0.0"}}'
# Expected: 202 Accepted

# Step 6: Verify replication on Node 2
sleep 2
curl http://node2:8787/events | jq '.[] | select(.event_type == "deploy")'
# Expected: event appears in Node 2's event log (replicated via NATS)

# Step 7: Cache invalidation
curl -X POST http://node1:8787/cdn/flush -H "Authorization: Bearer sk-node"
# Expected: 200 OK, Node 2's cache also invalidated via NATS

# Step 8: Both nodes serving traffic
for node in node1 node2; do
  curl http://$node:8787/healthz && echo " $node OK"
done
# Expected: both 200 OK

# Step 9: Verify event store durability
systemctl restart portail
sleep 2
EVENTS_AFTER=$(curl -s http://node1:8787/events | jq 'length')
# Expected: EVENTS_AFTER >= EVENTS_BEFORE (persisted to disk)
```

### Expected Outcomes

| Check | What to verify |
|-------|---------------|
| Nix install | Binary on PATH on both architectures |
| NATS cluster | Node 1 runs NATS, Node 2 connects |
| Event replication | Events published on Node 1 appear on Node 2 within 2s |
| Cache invalidation | Flush on Node 1 propagates to Node 2 |
| Crash recovery | Events survive portail restart (SQLite WAL) |
| Multi-arch | Works on x86_64 AND aarch64 |

---

## Scenario 4: Kubernetes — Helm + Ingress + HPA

### Context

An enterprise runs Portail in a Kubernetes cluster with:
- Nginx Ingress for TLS termination
- HPA for auto-scaling
- Redis for shared cache
- NATS for event bridge
- Prometheus for metrics scraping

### Target State

```
┌──────────────────────────────────────────────────────────┐
│                   Kubernetes Cluster                     │
│                                                          │
│  ┌─────────────────┐  ┌──────────┐  ┌──────────┐        │
│  │  Ingress (TLS)   │  │  Redis   │  │  NATS    │        │
│  │  api.example.com │  │  :6379   │  │  :4222   │        │
│  └────────┬────────┘  └────┬─────┘  └────┬─────┘        │
│           │                 │              │               │
│  ┌────────┴─────────────────┴──────────────┴─────┐       │
│  │           portail Deployment (HPA 2-10)       │       │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐    │       │
│  │  │  Pod 1   │  │  Pod 2   │  │  Pod 3   │    │       │
│  │  │  portail │  │  portail │  │  portail │    │       │
│  │  └──────────┘  └──────────┘  └──────────┘    │       │
│  └───────────────────────────────────────────────┘       │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐                      │
│  │  Prometheus  │  │   Grafana    │                      │
│  │  (scrape)    │  │  (dashboard) │                      │
│  └──────────────┘  └──────────────┘                      │
└──────────────────────────────────────────────────────────┘
```

### Flow

```bash
# Step 1: Add Helm repo (future — placeholder for v2.0)
# helm repo add portail https://charts.portail.dev
# helm install portail portail/portail -f values.yaml

# Step 2: Deploy via kubectl (for now)
kubectl apply -f deploy/k8s/
# Expected: deployment, service, ingress created

# Step 3: Verify pods
kubectl get pods -l app=portail
# Expected: 3/3 Running

# Step 4: Health check through ingress
curl https://api.example.com/healthz
# Expected: 200 OK

# Step 5: Verify HPA
kubectl get hpa portail
# Expected: current 3, min 2, max 10

# Step 6: Load test (trigger scale-up)
oha -n 10000 -c 100 https://api.example.com/v1/chat/completions \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}'
# Expected: pods scale up from 3 to ~6-8 under load

# Step 7: Verify metrics
kubectl port-forward svc/prometheus 9090:9090
curl http://localhost:9090/api/v1/query?query=portail_http_requests_total
# Expected: counter > 10000

# Step 8: Verify Redis cache
kubectl exec deploy/redis -- redis-cli DBSIZE
# Expected: keys > 0 (cache entries stored)

# Step 9: Rolling update
kubectl set image deploy/portail portail=ghcr.io/peterlodri-sec/portail:v2.0.0
kubectl rollout status deploy/portail
# Expected: 0 downtime, requests continue during roll

# Step 10: Pod kill test
kubectl delete pod -l app=portail --grace-period=30
sleep 5
kubectl get pods -l app=portail
# Expected: all pods recreated, health checks passing
```

### Expected Outcomes

| Check | What to verify |
|-------|---------------|
| Deploy works | Deployment, Service, Ingress created |
| Health check | Returns 200 through Ingress |
| HPA scales | Pods increase under load (>2) |
| Metrics scraped | Prometheus has portail metrics |
| Cache shared | Redis DBSIZE > 0 after traffic |
| Rolling update | Zero downtime during image change |
| Crash recovery | Pods recreate after delete |
| Multi-pod | All pods serve traffic (LB distributes) |

---

## Cross-Scenario Matrix

| Capability | MacBook | VPS/Docker | NixOS | Kubernetes |
|-----------|---------|------------|-------|------------|
| Install | curl pipe | docker pull | nix profile | helm/kubectl |
| TLS | Self-signed | Let's Encrypt | Self-signed | Ingress TLS |
| Cache | Moka only | Moka + Redis | Moka + Redis | Redis shared |
| Store | SQLite :memory: | SQLite disk + NATS | SQLite disk + NATS | SQLite disk + NATS |
| Auth | Disabled | API key | API key | API key (secret) |
| Rate limit | Yes | Yes | Yes | Yes |
| Observability | Dashboard + TUI | Dashboard + Prometheus | Dashboard | Prometheus + Grafana |
| Scaling | N/A | N/A | Manual (2 nodes) | HPA 2-10 |
| Multi-arch | aarch64 only | x86_64 | x86_64 + aarch64 | x86_64 |

---

## CI Integration — dev-cx53 (Self-Hosted Runner)

The self-hosted CI runner `dev-cx53` runs the Docker Compose scenario
(scenario 2) on every tag push and manual trigger.

### Workflow

```
.github/workflows/e2e-self-hosted.yml
  └─ docker-stack job (runs-on: self-hosted)
       ├─ Builds portail from source (cargo build --release)
       ├─ Creates docker-compose.yml with portail + Redis + NATS
       ├─ Starts stack (docker compose up -d --wait)
       ├─ Smoke tests: healthz, readyz, dashboard
       ├─ Auth test: verifies 401 on unauthenticated requests
       ├─ Rate limit test: verifies 429 responses after burst
       ├─ CLI smoke: portail --version
       ├─ Dashboard: jq validation of JSON structure
       ├─ NATS: connectivity check via :8222/varz
       ├─ Redis: PING via redis-cli
       └─ Cleanup: docker compose down -v

  └─ binary-smoke job (runs-on: self-hosted)
       ├─ Builds portail (cargo build --release)
       └─ Runs scripts/e2e-test.sh against localhost:18787
```

### What's tested (dev-cx53)

| Layer | Test |
|-------|------|
| Docker | Stack starts, all services healthy |
| API | /healthz, /readyz, /dashboard all 200 |
| Auth | 401 on unauthenticated /v1/* |
| Rate limit | 429 after burst exceeded |
| Redis | Connected, PING |
| NATS | Connected, monitoring endpoint |
| CLI | Binary build + version check |
| Config | TOML with Redis, NATS, auth, rate limit, store |

### What's NOT tested (dev-cx53 limitations)

- Multi-node NATS replication (single host)
- PostgreSQL backend (no alternative SQL engine)
- macOS-specific (no aarch64-darwin runner)
- Kubernetes (no k8s cluster)
- Load testing (separate benchmark-gate workflow)

### Trigger

- On every `v*` tag push (release candidates)
- Manual via `workflow_dispatch`
- Never on PR (only self-hosted, security-gated)

