# OTLP → Grafana Observability Spec

**Milestone:** P5 (v4)
**Status:** Spec
**Owner:** Peter Lodri

---

## Overview

Portail already exports OTLP traces via gRPC (`src/telemetry.rs`) and exposes Prometheus metrics at `/metrics`. This spec adds the infrastructure to visualize everything in Grafana via an OpenTelemetry Collector pipeline.

```
[portail] ──OTLP/gRPC:4317──▶ [otel-collector] ──▶ [Grafana Tempo]  (traces)
                                     │──▶ [Prometheus]                (metrics)
                                     │──▶ [Loki]                      (logs, optional)
```

---

## 1. OpenTelemetry Collector Config

**File:** `docker/otel-collector-config.yaml`

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

  # Scrape Prometheus metrics from Portail's /metrics endpoint
  prometheus:
    config:
      scrape_configs:
        - job_name: 'portail'
          scrape_interval: 10s
          static_configs:
            - targets: ['host.docker.internal:8787']

processors:
  batch:
    send_batch_size: 8192
    timeout: 5s

  # Enrich spans with resource attributes
  resource:
    attributes:
      - key: deployment.environment
        value: "production"
        action: upsert

exporters:
  # Traces → Grafana Tempo
  otlp/tempo:
    endpoint: tempo:4317
    tls:
      insecure: true

  # Metrics → Prometheus
  prometheus:
    endpoint: 0.0.0.0:8889
    namespace: portail
    const_labels:
      instance: "portail-prod"

  # Logs → Loki (optional)
  loki/loki:
    endpoint: http://loki:3100/loki/api/v1/push

  # Debug logging (remove in production)
  debug:
    verbosity: basic

extensions:
  health_check:
    endpoint: 0.0.0.0:13133
  zpages:
    endpoint: 0.0.0.0:55679

service:
  extensions: [health_check, zpages]
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch, resource]
      exporters: [otlp/tempo]
    metrics:
      receivers: [otlp, prometheus]
      processors: [batch]
      exporters: [prometheus]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [loki/loki]
```

---

## 2. Docker Compose — Observability Stack

**File:** `docker/docker-compose.observability.yml`

```yaml
version: "3.9"

services:
  # ── OpenTelemetry Collector ──────────────────────────────
  otel-collector:
    image: otel/opentelemetry-collector-contrib:0.102.0
    command: ["--config=/etc/otel/config.yaml"]
    volumes:
      - ./otel-collector-config.yaml:/etc/otel/config.yaml:ro
    ports:
      - "4317:4317"   # OTLP gRPC
      - "4318:4318"   # OTLP HTTP
      - "8889:8889"   # Prometheus metrics export
      - "13133:13133" # Health check
    depends_on:
      - tempo
      - prometheus
      - loki

  # ── Grafana ──────────────────────────────────────────────
  grafana:
    image: grafana/grafana:11.1.0
    environment:
      GF_SECURITY_ADMIN_USER: admin
      GF_SECURITY_ADMIN_PASSWORD: admin
      GF_USERS_ALLOW_SIGN_UP: "false"
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana/provisioning:/etc/grafana/provisioning:ro
      - ./grafana/dashboards:/var/lib/grafana/dashboards:ro
    ports:
      - "3000:3000"
    depends_on:
      - prometheus
      - tempo

  # ── Prometheus (metrics backend) ────────────────────────
  prometheus:
    image: prom/prometheus:v2.53.0
    command:
      - "--config.file=/etc/prometheus/prometheus.yml"
      - "--storage.tsdb.retention.time=15d"
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    ports:
      - "9090:9090"

  # ── Tempo (traces backend) ──────────────────────────────
  tempo:
    image: grafana/tempo:2.5.1
    command: ["-config.file=/etc/tempo/tempo.yaml"]
    volumes:
      - ./tempo/tempo.yaml:/etc/tempo/tempo.yaml:ro
      - tempo-data:/var/tempo
    ports:
      - "3200:3200"   # Tempo HTTP
      - "4317"        # OTLP gRPC

  # ── Loki (logs backend, optional) ──────────────────────
  loki:
    image: grafana/loki:3.1.0
    command: ["-config.file=/etc/loki/loki.yaml"]
    volumes:
      - ./loki/loki.yaml:/etc/loki/loki.yaml:ro
      - loki-data:/loki
    ports:
      - "3100:3100"

volumes:
  grafana-data:
  prometheus-data:
  tempo-data:
  loki-data:
```

### Companion configs

**`docker/prometheus/prometheus.yml`**

```yaml
global:
  scrape_interval: 10s
  evaluation_interval: 10s

scrape_configs:
  - job_name: 'otel-collector'
    static_configs:
      - targets: ['otel-collector:8889']

  - job_name: 'portail'
    static_configs:
      - targets: ['host.docker.internal:8787']

  - job_name: 'otel-collector-telemetry'
    static_configs:
      - targets: ['otel-collector:8888']
```

**`docker/tempo/tempo.yaml`**

```yaml
server:
  http_listen_port: 3200

distributor:
  receivers:
    otlp:
      protocols:
        grpc:
          endpoint: 0.0.0.0:4317

metrics_generator:
  registry:
    external_labels:
      source: tempo
      cluster: docker-compose
  storage:
    path: /var/tempo/generator/wal
    remote_write:
      - url: http://prometheus:9090/api/v1/write
        send_exemplars: true
  traces_storage:
    path: /var/tempo/generator/traces

storage:
  trace:
    backend: local
    local:
      path: /var/tempo/traces

compactor:
  compaction:
    block:
      live_objects: 10000
```

**`docker/loki/loki.yaml`**

```yaml
auth_enabled: false

server:
  http_listen_port: 3100

common:
  path_prefix: /loki
  storage:
    filesystem:
      chunks_directory: /loki/chunks
      rules_directory: /loki/rules
  replication_factor: 1

schema_config:
  configs:
    - from: "2024-01-01"
      store: tsdb
      object_store: filesystem
      schema: v13
      index:
        prefix: index_
        period: 24h
```

**`docker/grafana/provisioning/datasources/datasources.yaml`**

```yaml
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true

  - name: Tempo
    type: tempo
    access: proxy
    url: http://tempo:3200
    uid: tempo
    jsonData:
      tracesToLogsV2:
        datasourceUid: loki
        filterByTraceID: true
        filterBySpanID: true
      tracesToMetrics:
        datasourceUid: prometheus
      serviceMap:
        datasourceUid: prometheus
      nodeGraph:
        enabled: true

  - name: Loki
    type: loki
    access: proxy
    url: http://loki:3100
    uid: loki
```

**`docker/grafana/provisioning/dashboards/dashboards.yaml`**

```yaml
apiVersion: 1

providers:
  - name: 'Portail'
    orgId: 1
    folder: ''
    type: file
    disableDeletion: false
    editable: true
    options:
      path: /var/lib/grafana/dashboards
      foldersFromFilesStructure: false
```

---

## 3. Grafana Dashboard JSON

**File:** `docker/grafana/dashboards/portail.json`

```json
{
  "uid": "portail-overview",
  "title": "Portail Gateway",
  "tags": ["portail", "gateway"],
  "timezone": "browser",
  "refresh": "10s",
  "time": { "from": "now-1h", "to": "now" },
  "templating": {
    "list": [
      {
        "name": "datasource",
        "type": "datasource",
        "query": "prometheus",
        "current": { "selected": true, "text": "Prometheus", "value": "Prometheus" }
      }
    ]
  },
  "panels": [
    {
      "title": "Request Rate",
      "type": "timeseries",
      "gridPos": { "h": 8, "w": 12, "x": 0, "y": 0 },
      "targets": [
        {
          "expr": "sum(rate(portail_http_requests_total[5m]))",
          "legendFormat": "Total",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        },
        {
          "expr": "sum(rate(portail_http_requests_total{status=~\"5..\"}[5m]))",
          "legendFormat": "Errors (5xx)",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "reqps",
          "custom": { "drawStyle": "line", "fillOpacity": 10 }
        }
      }
    },
    {
      "title": "Request Latency (p50 / p95 / p99)",
      "type": "timeseries",
      "gridPos": { "h": 8, "w": 12, "x": 12, "y": 0 },
      "targets": [
        {
          "expr": "histogram_quantile(0.50, sum(rate(portail_http_request_duration_seconds_bucket[5m])) by (le))",
          "legendFormat": "p50",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        },
        {
          "expr": "histogram_quantile(0.95, sum(rate(portail_http_request_duration_seconds_bucket[5m])) by (le))",
          "legendFormat": "p95",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        },
        {
          "expr": "histogram_quantile(0.99, sum(rate(portail_http_request_duration_seconds_bucket[5m])) by (le))",
          "legendFormat": "p99",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s",
          "custom": { "drawStyle": "line", "fillOpacity": 10 }
        }
      }
    },
    {
      "title": "Error Rate (%)",
      "type": "gauge",
      "gridPos": { "h": 8, "w": 8, "x": 0, "y": 8 },
      "targets": [
        {
          "expr": "sum(rate(portail_http_requests_total{status=~\"5..\"}[5m])) / sum(rate(portail_http_requests_total[5m])) * 100",
          "legendFormat": "Error %",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "percent",
          "min": 0,
          "max": 100,
          "thresholds": {
            "mode": "absolute",
            "steps": [
              { "color": "green", "value": null },
              { "color": "yellow", "value": 1 },
              { "color": "red", "value": 5 }
            ]
          }
        }
      }
    },
    {
      "title": "Active Connections",
      "type": "stat",
      "gridPos": { "h": 8, "w": 8, "x": 8, "y": 8 },
      "targets": [
        {
          "expr": "portail_active_connections",
          "legendFormat": "Active",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "mode": "absolute",
            "steps": [
              { "color": "green", "value": null },
              { "color": "yellow", "value": 100 },
              { "color": "red", "value": 500 }
            ]
          }
        }
      }
    },
    {
      "title": "Cache Hit Ratio",
      "type": "gauge",
      "gridPos": { "h": 8, "w": 8, "x": 16, "y": 8 },
      "targets": [
        {
          "expr": "rate(portail_cache_hits_total[5m]) / (rate(portail_cache_hits_total[5m]) + rate(portail_cache_misses_total[5m])) * 100",
          "legendFormat": "Hit %",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "percent",
          "min": 0,
          "max": 100,
          "thresholds": {
            "mode": "absolute",
            "steps": [
              { "color": "red", "value": null },
              { "color": "yellow", "value": 50 },
              { "color": "green", "value": 80 }
            ]
          }
        }
      }
    },
    {
      "title": "Rate Limit Rejections",
      "type": "timeseries",
      "gridPos": { "h": 8, "w": 12, "x": 0, "y": 16 },
      "targets": [
        {
          "expr": "sum(rate(portail_rate_limit_rejected_total[5m])) by (key)",
          "legendFormat": "{{key}}",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "reqps",
          "custom": { "drawStyle": "bars", "fillOpacity": 50 }
        }
      }
    },
    {
      "title": "AI Gateway Requests",
      "type": "timeseries",
      "gridPos": { "h": 8, "w": 12, "x": 12, "y": 16 },
      "targets": [
        {
          "expr": "sum(rate(portail_ai_gateway_requests_total[5m])) by (status)",
          "legendFormat": "{{status}}",
          "datasource": { "type": "prometheus", "uid": "${datasource}" }
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "reqps",
          "custom": { "drawStyle": "line", "fillOpacity": 20 }
        }
      }
    },
    {
      "title": "Trace Explorer",
      "type": "traces",
      "gridPos": { "h": 8, "w": 24, "x": 0, "y": 24 },
      "targets": [
        {
          "queryType": "traceqlSearch",
          "limit": 20,
          "filters": [
            {
              "id": "serviceName",
              "tag": "service.name",
              "operator": "=",
              "scope": "resource"
            }
          ]
        }
      ],
      "datasource": { "type": "tempo", "uid": "tempo" }
    }
  ],
  "schemaVersion": 39
}
```

---

## 4. Portail Config Changes

Add to `portail.toml` (or `config.toml`):

```toml
[telemetry]
enabled = true
endpoint = "http://localhost:4317"  # OTel Collector gRPC endpoint
service_name = "portail"
sampling_ratio = 0.1  # 10% of traces in production, 1.0 in dev
```

### Additional metrics to emit

Portail already emits `http_requests_total`, `http_request_duration_seconds`, and `ai_gateway_requests_total`. Add these counters/histograms to cover the dashboard panels:

```rust
// src/proxy.rs — add to existing metrics_middleware
counter!("active_connections").increment(1);
// ... in the response path:
counter!("active_connections").decrement(1);

// src/plugins/redis_cache.rs or file_cache.rs
counter!("cache_hits_total").increment(1);
counter!("cache_misses_total").increment(1);

// src/rate_limit.rs — on rejection
counter!("rate_limit_rejected_total", "key" => key.clone()).increment(1);
```

For a quick start, the Prometheus scrape of `/metrics` gives all existing counters. The new `active_connections` gauge and `cache_*` / `rate_limit_*` counters can be added incrementally.

---

## 5. Alert Rules

**File:** `docker/prometheus/alert_rules.yml`

```yaml
groups:
  - name: portail
    rules:
      - alert: HighErrorRate
        expr: |
          sum(rate(portail_http_requests_total{status=~"5.."}[5m]))
          / sum(rate(portail_http_requests_total[5m]))
          > 0.05
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Portail error rate > 5%"
          description: "{{ $value | humanizePercentage }} of requests are 5xx"

      - alert: HighP99Latency
        expr: |
          histogram_quantile(0.99,
            sum(rate(portail_http_request_duration_seconds_bucket[5m])) by (le)
          ) > 2
        for: 3m
        labels:
          severity: warning
        annotations:
          summary: "Portail p99 latency > 2s"
          description: "p99 is {{ $value }}s"

      - alert: HighDiskUsage
        expr: |
          (1 - node_filesystem_avail_bytes{mountpoint="/"} / node_filesystem_size_bytes{mountpoint="/"}) * 100 > 80
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Disk usage > 80%"
          description: "{{ $value | humanizePercentage }} disk used"

      - alert: ServiceDown
        expr: up{job="portail"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Portail instance is down"
```

---

## 6. Setup Instructions

### Quick start

```bash
# 1. Start the observability stack
cd docker
docker compose -f docker-compose.observability.yml up -d

# 2. Enable telemetry in Portail config
cat >> portail.toml << 'EOF'
[telemetry]
enabled = true
endpoint = "http://localhost:4317"
service_name = "portail"
sampling_ratio = 1.0
EOF

# 3. Start Portail
cargo run -- serve

# 4. Open Grafana
open http://localhost:3000   # admin / admin
# Dashboard: Portail Gateway (auto-provisioned)
```

### Production deployment

```bash
# Set sampling to 10% in production
sed -i 's/sampling_ratio = 1.0/sampling_ratio = 0.1/' portail.toml

# Use host networking if deploying outside Docker
# Update otel-collector-config.yaml target to the real Portail host
```

### Verifying the pipeline

```bash
# Check OTel Collector health
curl http://localhost:13133/

# Check Prometheus targets
curl http://localhost:9090/api/v1/targets

# Check Tempo traces
curl http://localhost:3200/ready

# Generate test traffic
for i in $(seq 1 100); do
  curl -s http://localhost:8787/health > /dev/null &
done
wait

# Verify data in Grafana
# → Portail Gateway dashboard should show request rate, latency, errors
```

---

## 7. Files to Create

| File | Purpose |
|------|---------|
| `docker/otel-collector-config.yaml` | OTel Collector pipeline config |
| `docker/docker-compose.observability.yml` | Docker Compose for the stack |
| `docker/prometheus/prometheus.yml` | Prometheus scrape config |
| `docker/prometheus/alert_rules.yml` | Alertmanager rules |
| `docker/tempo/tempo.yaml` | Tempo config |
| `docker/loki/loki.yaml` | Loki config |
| `docker/grafana/provisioning/datasources/datasources.yaml` | Auto-provisioned datasources |
| `docker/grafana/provisioning/dashboards/dashboards.yaml` | Dashboard loader config |
| `docker/grafana/dashboards/portail.json` | Portail Gateway dashboard |

## 8. Implementation Order

1. Create `docker/` directory structure and all config files
2. Add new metrics (`active_connections`, `cache_hits_total`, `cache_misses_total`, `rate_limit_rejected_total`) to `src/proxy.rs`, `src/rate_limit.rs`
3. `docker compose up` and verify Grafana shows data
4. Tune dashboard panels to match actual metric names/labels
5. Add Alertmanager config if email/Slack alerts needed
