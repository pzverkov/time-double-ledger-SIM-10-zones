# Security Policy

## Supported versions

This is a simulation/demo project. Security fixes are applied to the latest
`main` only; there are no long-term support branches.

## Reporting a vulnerability

Please report suspected vulnerabilities privately via GitHub Security Advisories
("Report a vulnerability" on the repository's Security tab) rather than opening a
public issue. Include a description, affected component (Go backend, Rust backend,
web, infra), and reproduction steps. Expect an initial response within a few days.

## Scope and notes

This is a demo, not a hardened production deployment. Known posture:

- **Default credentials** in `infra/docker-compose.yml` (`postgres/postgres`,
  `admin/admin`, `dev-admin-key`) are for local development only and must be
  overridden in any shared environment.
- **Message bus** (NATS, Redpanda) runs PLAINTEXT with no auth in the dev stack;
  a real deployment needs SASL/TLS and credentials.
- **No rate limiting** on the public API endpoints in the dev configuration.
- Operator endpoints (zone controls, snapshot/restore) require the `ADMIN_KEY`
  header.
- Internal error details are logged server-side and never returned to API clients.

Automated scanning runs in CI: CodeQL, `govulncheck` (Go), `cargo audit` (Rust),
and `npm audit` (web).
