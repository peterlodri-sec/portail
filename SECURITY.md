# Security Policy

## Self-Hosted Runner Security Model

### Overview

Portail uses self-hosted runners for Linux builds. This document explains the security model and access controls.

### Who Can Trigger CI?

| Trigger | Runner | Who Can Do It |
|---------|--------|---------------|
| Push to `main` | Self-hosted | Maintainers, agents |
| PR from same repo | Self-hosted | Collaborators |
| PR from fork | GitHub-hosted | Anyone |
| Tag push (`v*`) | Self-hosted | Maintainers only |
| Manual dispatch | Self-hosted | Maintainers only |

### Security Controls

1. **Fork Detection**
   - PRs from forks automatically use GitHub-hosted runners
   - Self-hosted runners are NEVER exposed to fork PRs
   - This prevents arbitrary code execution on our infrastructure

2. **Tag Protection**
   - Only maintainers can push version tags
   - Tags must match `vX.Y.Z` format
   - Release workflow validates tag format before proceeding

3. **Branch Protection**
   - `main` branch requires PR reviews
   - Status checks must pass before merge
   - Force pushes are disabled

4. **Secrets Management**
   - `CARGO_REGISTRY_TOKEN` is stored as a GitHub secret
   - Secrets are NOT exposed to fork PRs
   - Cosign uses keyless signing (OIDC)

### Agent Access

Agents (Claude Code, OpenCode, etc.) can:
- Push to feature branches
- Create PRs
- Trigger CI on same-repo PRs
- NOT push to `main` directly
- NOT push version tags

### Making the Repo Public

When making the repo public:

1. **Keep self-hosted runners secure**
   - Fork PRs will automatically use GitHub-hosted runners
   - No configuration needed — the workflow handles this

2. **Configure branch protection**
   - Settings → Branches → Add rule for `main`
   - Require PR reviews (1+ approvals)
   - Require status checks
   - Disable force pushes

3. **Configure tag protection**
   - Settings → Tags → Add rule for `v*`
   - Restrict to maintainers only

4. **Enable security features**
   - Settings → Security → Enable Dependabot
   - Settings → Security → Enable code scanning
   - Settings → Security → Enable secret scanning

### Runner Isolation

Self-hosted runners are isolated via:
- Dedicated user account (`dev`)
- Limited filesystem access
- No access to production secrets
- Automatic cleanup after each job

### Monitoring

All CI runs are logged and can be audited:
- GitHub Actions logs
- Runner system logs
- Portail event log (for agent activity)

## Reporting Vulnerabilities

If you discover a security vulnerability, please report it privately to:
- GitHub: peterlodri-sec
- Do NOT open a public issue

We will respond within 48 hours and provide a fix timeline.
