# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes |
| < 0.1   | No |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately:

1. **GitHub**: Open a private security advisory
2. **Email**: (add your email)

Do NOT open a public issue for security vulnerabilities.

## Response Time

We will respond within 48 hours and provide a fix timeline.

## Security Measures

- All dependencies audited with `cargo audit`
- Signed releases with cosign (keyless via OIDC)
- HSTS enabled on all endpoints
- Input validation on all user-facing APIs
- Rate limiting (planned for v0.2.0)
