# TLS + Deployment Guide

Portail runs behind a reverse proxy for TLS termination. Two recommended setups.

## Option 1: Caddy (auto HTTPS)

```caddyfile
portail.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

Caddy auto-provisions Let's Encrypt certificates. Zero config beyond DNS.

## Option 2: Nginx + certbot

```nginx
server {
    listen 443 ssl;
    server_name portail.example.com;
    ssl_certificate /etc/letsencrypt/live/portail.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/portail.example.com/privkey.pem;
    location / {
        proxy_pass http://127.0.0.1:8787;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Option 3: NixOS (systemd + ACME)

```nix
{ config, pkgs, ... }: {
  services.portail = {
    enable = true;
    listen = "127.0.0.1:8787";
  };
  security.acme.certs."portail.example.com".email = "admin@example.com";
  services.nginx = {
    enable = true;
    virtualHosts."portail.example.com" = {
      forceSSL = true;
      enableACME = true;
      locations."/" = { proxyPass = "http://127.0.0.1:8787"; };
    };
  };
}
```

## Load Testing

```bash
# Install
cargo install drill

# Basic throughput test
drill --benchmark /dev/stdin << 'EOF'
{
  "concurrency": 50,
  "requests": 1000,
  "steps": [{
    "url": "http://127.0.0.1:8787/healthz"
  }]
}
EOF

# Full pipeline
cargo install httplz
httplz --concurrency 100 --requests 5000 --url http://127.0.0.1:8787/v1/chat/completions --method POST \
  --header "Content-Type: application/json" \
  --body '{"model":"test","messages":[{"role":"user","content":"ping"}]}'
```

## Health Check Endpoints

| Endpoint | Purpose |
|----------|---------|
| `/healthz` | Liveness — always 200 |
| `/readyz` | Readiness — checks upstream |
| `/livez` | Alias for healthz |
| `/dashboard` | JSON health snapshot |
| `/metrics` | Prometheus metrics |
