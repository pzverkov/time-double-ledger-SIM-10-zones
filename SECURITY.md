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
  overridden in any shared environment. Both backends refuse to start when
  `APP_ENV=production` and `ADMIN_KEY` is unset or a known weak default.
- **Message bus** (NATS, Redpanda) runs PLAINTEXT with no auth in the dev stack.
  Auth and TLS are configurable: NATS user/password and `tls://` are carried by
  `NATS_URL`, and `NATS_CREDS` points at a JWT/nkey credentials file.
- **Rate limiting**: the public write path (`POST /v1/transfers`) has a per-client
  token-bucket limiter (`RATE_LIMIT_RPS`, default 50, `0` disables) returning 429.
  It keys on the peer address by default; `X-Forwarded-For`/`X-Real-IP` are honored
  only when `TRUST_PROXY_HEADERS=true`, so a direct caller cannot spoof the key to
  bypass the limit. This is a backstop; a real edge deployment should also
  rate-limit at the gateway.
- Operator endpoints (zone controls, snapshot/restore) require the `ADMIN_KEY`
  header.
- Internal error details are logged server-side and never returned to API clients.

Automated scanning runs in CI: CodeQL, `govulncheck` (Go), `cargo audit` (Rust),
and `npm audit` (web).

## Production hardening checklist

Before exposing this beyond a local machine:

- Set `APP_ENV=production` so the backends reject a weak/unset `ADMIN_KEY`.
- Replace every default in `infra/.env.example` with strong, unique secrets
  (`POSTGRES_PASSWORD`, `ADMIN_KEY`, `GRAFANA_ADMIN_PASSWORD`) sourced from a
  secret manager, not committed to the repo.
- Enable broker auth/TLS: supply `NATS_CREDS` or an authenticated `NATS_URL`
  (`tls://`); for Redpanda, configure SASL/TLS on the brokers.
- Require TLS to Postgres (`sslmode=require` or stricter in `DATABASE_URL`).
- Put the public API behind a gateway that enforces rate limiting and TLS.
