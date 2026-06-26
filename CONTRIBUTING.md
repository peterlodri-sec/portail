# Contributing to Portail

## For Humans

### Getting Started

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Run tests: `cargo test`
5. Run linting: `cargo clippy -- -D warnings`
6. Submit a pull request

### Code Review

All PRs require:
- Passing CI checks
- At least 1 approval from a maintainer
- No merge conflicts

### Branch Protection

- `main` branch is protected
- Force pushes are disabled
- Status checks must pass

## For AI Agents

### Supported Agents

- Claude Code
- OpenCode
- Codex
- GitHub Copilot

### Agent Workflow

1. **Create a feature branch**
   ```bash
   git checkout -b agent/feature-name
   ```

2. **Make changes and test**
   ```bash
   cargo test
   cargo clippy -- -D warnings
   ```

3. **Commit with agent prefix**
   ```bash
   git commit -m "agent: description of changes"
   ```

4. **Push and create PR**
   ```bash
   git push origin agent/feature-name
   gh pr create --title "agent: feature name" --body "Description"
   ```

### Agent Permissions

Agents can:
- Push to feature branches
- Create PRs
- Trigger CI on same-repo PRs
- Comment on issues and PRs

Agents cannot:
- Push to `main` directly
- Push version tags
- Access production secrets
- Modify branch protection rules

### Triggering Agent Builds

Agents can trigger builds via repository_dispatch:

```bash
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/peterlodri-sec/portail/dispatches \
  -d '{
    "event_type": "agent-build",
    "client_payload": {
      "agent_id": "claude-code",
      "feature": "my-feature"
    }
  }'
```

### Security Model

- Fork PRs use GitHub-hosted runners (safe)
- Same-repo PRs use self-hosted runners (trusted)
- Only maintainers can merge to `main`
- Only maintainers can push tags

## CI/CD Pipeline

### Workflows

| Workflow | Trigger | Runner | Purpose |
|----------|---------|--------|---------|
| `ci.yml` | Push, PR | Self-hosted/GitHub | Build, test, lint |
| `release.yml` | Tag push | Self-hosted | Build release binaries |
| `docker.yml` | Tag push | Self-hosted | Build Docker image |
| `agent-build.yml` | Repository dispatch | Self-hosted | Agent-triggered builds |

### Self-Hosted Runners

Self-hosted runners are used for:
- Linux builds (x86_64)
- Release builds
- Agent-triggered builds

GitHub-hosted runners are used for:
- macOS builds
- Fork PR builds (security)

### Secrets

| Secret | Purpose | Who Has Access |
|--------|---------|----------------|
| `CARGO_REGISTRY_TOKEN` | Publish to crates.io | Maintainers |
| `GITHUB_TOKEN` | GitHub API | Automatic |

## Release Process

1. Update `CHANGELOG.md`
2. Update version in `Cargo.toml`
3. Create and push tag: `git tag v0.1.0 && git push --tags`
4. GitHub Actions will:
   - Build release binaries
   - Sign with cosign
   - Create GitHub Release
   - Publish to crates.io

## Questions?

- Open an issue for bugs
- Start a discussion for questions
- Contact maintainers for security issues
