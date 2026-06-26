# Auto-TinyURL Plugin

## Overview

Automatic URL shortening for internal network services. Any URL passed through portail gets a short alias that persists for 24 hours (configurable).

## Quick Start

```bash
# Shorten a URL
curl -X POST http://localhost:8787/tinyurl/shorten \
  -H "Content-Type: application/json" \
  -d '{"url": "https://internal-service.local:8080/very/long/path?with=params"}'

# Response:
# {
#   "id": "abc123",
#   "original_url": "https://internal-service.local:8080/very/long/path?with=params",
#   "short_url": "http://localhost:8787/s/abc123",
#   "created_at": 1719412800,
#   "expires_at": 1719499200,
#   "hits": 0
# }

# Use short URL (301 redirect)
curl -L http://localhost:8787/s/abc123

# Get stats
curl http://localhost:8787/tinyurl/stats
```

## Integration with AI Coding Tools

### Claude Code

Add to your `.claude/settings.json`:

```json
{
  "tools": {
    "portail_tinyurl": {
      "command": "curl",
      "args": [
        "-X", "POST",
        "http://localhost:8787/tinyurl/shorten",
        "-H", "Content-Type: application/json",
        "-d", "{\"url\": \"$URL\"}"
      ],
      "description": "Shorten internal URLs for sharing"
    }
  }
}
```

Usage in Claude Code:
```
> Shorten this URL: https://internal-api.local:8443/v1/agents/config

Claude will call portail_tinyurl and return: http://localhost:8787/s/xyz789
```

### OpenCode

Add to `opencode.json`:

```json
{
  "mcp": {
    "servers": {
      "portail": {
        "command": "portail",
        "args": ["serve"],
        "env": {
          "PORTAIL_LISTEN": "127.0.0.1:8787"
        }
      }
    }
  }
}
```

Then use in OpenCode:
```
> Shorten this URL for my teammate: https://grafana.local/d/abc123/dashboard

OpenCode calls portail MCP endpoint and returns the short URL.
```

### Whale

Add to whale config:

```yaml
plugins:
  portail:
    url: http://localhost:8787
    endpoints:
      tinyurl: /tinyurl/shorten
```

Usage:
```
whale shorten https://internal-service.local/long/path
```

## E2E Guide: Setting Up Internal URL Shortening

### Step 1: Start Portail

```bash
portail serve --config portail.toml
```

### Step 2: Configure Your Domain (Optional)

```bash
# With custom domain
portail setup --domain links.yourcompany.com

# Or use localhost
portail setup --self-signed
```

### Step 3: Configure TinyURL in portail.toml

```toml
[tinyurl]
enabled = true
base_url = "https://links.yourcompany.com"
ttl_secs = 86400  # 24 hours
max_entries = 100000
secret = "your-secret-key-here"
```

### Step 4: Use in Your Workflow

#### From CLI

```bash
# Create short URL
SHORT=$(curl -s -X POST http://localhost:8787/tinyurl/shorten \
  -H "Content-Type: application/json" \
  -d '{"url": "https://internal-api.local:8443/v1/health"}' | jq -r '.short_url')

echo "Share this: $SHORT"
```

#### From Scripts

```bash
#!/bin/bash
# shorten.sh - Shorten a URL and copy to clipboard

URL="$1"
if [ -z "$URL" ]; then
  echo "Usage: shorten.sh <url>"
  exit 1
fi

SHORT=$(curl -s -X POST http://localhost:8787/tinyurl/shorten \
  -H "Content-Type: application/json" \
  -d "{\"url\": \"$URL\"}" | jq -r '.short_url')

echo "$SHORT" | pbcopy  # macOS
# echo "$SHORT" | xclip -selection clipboard  # Linux
echo "Copied: $SHORT"
```

#### From Docker

```bash
# Shorten URL from inside Docker network
docker exec portail curl -s -X POST http://localhost:8787/tinyurl/shorten \
  -H "Content-Type: application/json" \
  -d '{"url": "http://api:8080/endpoint"}'
```

### Step 5: Monitor Usage

```bash
# Check stats
curl http://localhost:8787/tinyurl/stats

# Response:
# {
#   "total_entries": 42,
#   "active_entries": 38,
#   "expired_entries": 4,
#   "total_hits": 156
# }
```

## Use Cases

### 1. Sharing Internal Dashboards

```bash
# Long Grafana URL
GRAFANA="https://grafana.internal:3000/d/abc123/my-dashboard?orgId=1&from=now-6h&to=now"

# Shorten
SHORT=$(curl -s -X POST http://localhost:8787/tinyurl/shorten \
  -H "Content-Type: application/json" \
  -d "{\"url\": \"$GRAFANA\"}" | jq -r '.short_url')

# Share in Slack: "Check this dashboard: $SHORT"
```

### 2. API Documentation Links

```bash
# Shorten API docs
curl -X POST http://localhost:8787/tinyurl/shorten \
  -d '{"url": "https://api-docs.internal/v2/endpoints#auth"}'
```

### 3. CI/CD Pipeline Links

```yaml
# .github/workflows/deploy.yml
- name: Shorten build URL
  run: |
    SHORT=$(curl -s -X POST http://portail:8787/tinyurl/shorten \
      -H "Content-Type: application/json" \
      -d "{\"url\": \"${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}\"}" \
      | jq -r '.short_url')
    echo "Build URL: $SHORT" >> $GITHUB_STEP_SUMMARY
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  Auto-TinyURL Architecture                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   Client                                                     │
│      │                                                       │
│      ▼                                                       │
│   POST /tinyurl/shorten                                      │
│      │                                                       │
│      ▼                                                       │
│   ┌──────────────────────────────────────────────────────┐   │
│   │  Generate ID: base62(hash(url + secret))             │   │
│   └──────────────────────────────────────────────────────┘   │
│      │                                                       │
│      ▼                                                       │
│   ┌──────────────────────────────────────────────────────┐   │
│   │  Store: FxHashMap<String, TinyUrlEntry>               │   │
│   │  - Evict oldest if at capacity                       │   │
│   │  - Set TTL (default 24h)                             │   │
│   └──────────────────────────────────────────────────────┘   │
│      │                                                       │
│      ▼                                                       │
│   Return: { id, short_url, original_url, expires_at }        │
│                                                              │
│   GET /s/{id}                                                │
│      │                                                       │
│      ▼                                                       │
│   ┌──────────────────────────────────────────────────────┐   │
│   │  Lookup by ID                                        │   │
│   │  - Check expiration                                  │   │
│   │  - Increment hit counter                             │   │
│   └──────────────────────────────────────────────────────┘   │
│      │                                                       │
│      ▼                                                       │
│   301 Redirect → original_url                                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `enabled` | `true` | Enable/disable plugin |
| `base_url` | `http://localhost:8787` | Base URL for short links |
| `ttl_secs` | `86400` | Time-to-live in seconds (24h) |
| `max_entries` | `100000` | Maximum stored entries |
| `secret` | `portail-tinyurl-secret` | Secret for hash generation |

## API Reference

### POST /tinyurl/shorten

Create a short URL.

**Request:**
```json
{
  "url": "https://example.com/long/path"
}
```

**Response (201):**
```json
{
  "id": "abc123",
  "original_url": "https://example.com/long/path",
  "short_url": "http://localhost:8787/s/abc123",
  "created_at": 1719412800,
  "expires_at": 1719499200,
  "hits": 0
}
```

### GET /s/{id}

Resolve a short URL (301 redirect).

### GET /tinyurl/stats

Get usage statistics.

**Response:**
```json
{
  "total_entries": 42,
  "active_entries": 38,
  "expired_entries": 4,
  "total_hits": 156
}
```
