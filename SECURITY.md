# Security Policy

## Reporting a Vulnerability

Portail handles AI API traffic and may be deployed at the edge of your
infrastructure. If you discover a security vulnerability, please report it
privately by opening a security advisory at:

  https://github.com/peterlodri-sec/portail/security/advisories

Do **not** report security vulnerabilities via public GitHub issues.

## Scope

Security issues include but are not limited to:

- Authentication bypass or privilege escalation
- Unauthorised access to upstream AI/MCP/CDN services
- Injection attacks through API paths or MCP tool arguments
- Side-channel leaks via cache timing or error messages
- Dependency vulnerabilities with CVSS ≥ 7.0

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅ Active development — security fixes in next release |

## Disclosure

We aim to acknowledge receipt within 48 hours and provide an initial assessment
within 5 business days. Fixes are prioritised based on severity.
