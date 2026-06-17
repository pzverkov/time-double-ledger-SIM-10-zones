# Time Ledger Sim (Go + Rust)

A production-flavored simulation backend for a "time-currency" double-entry ledger with:
- Double-entry ledger (postings), integer units (1 unit = 1 second)
- 10 worldwide zones with operator-controlled status (`OK/DEGRADED/DOWN`)
- Zone controls: writes blocking, cross-zone throttle (0-100%), spool-and-replay
- Fraud/ops incidents per zone with ACK/ASSIGN/RESOLVE lifecycle
- Deterministic throttling via FNV-1a hashing (cross-language parity between Go and Rust)
- **At-least-once** messaging with **Transactional Outbox** + **Inbox dedup**, behind a swappable broker seam (NATS JetStream default, Redpanda/Kafka-API via `--features redpanda`); two consumer groups (fraud + analytics); versioned event schema and a dead-letter queue for poison events. See [ADR 0001](docs/adr/0001-messaging-broker.md)
- Observability: structured logs, Prometheus metrics, OpenTelemetry traces (Jaeger) with trace context propagated across the broker
- Security: admin-key-guarded operator actions with the audit actor derived from the authenticated request, per-client rate limiting on the public write path, configurable broker auth (NATS credentials), and a production startup guard that refuses weak/unset credentials
- Reliability: continuous ledger reconciliation (balance-vs-postings drift gauges), outbox/inbox/DLQ retention, a bounded recycling DB pool, request/statement timeouts, and a dependency-aware `/readyz` probe

Two backends sharing one HTTP API (validated in CI by Schemathesis). The Rust
backend is ahead in messaging and periodic ops jobs; see the [parity matrix](docs/parity-matrix.md).
- `go/` (Go 1.26+) - primary implementation
- `rust/sim/` (Rust edition 2024) - modular implementation, ahead on messaging/ops

## Dashboard (web/)

A React operator console with:
- SVG zone map with blast-radius visualization
- Connection health indicator (persistent offline banner, auto-reconnect)
- Periodic polling with visibility-aware refresh
- Controls, spool, incidents, audit trail, transfer generator

```bash
cd web && npm install && npm run dev
```

Open: http://localhost:5173

## Quickstart

Requirements: Docker + Docker Compose, Go 1.26+ and/or Rust stable.

```bash
just infra-up        # start Postgres, NATS, Flyway, both backends, observability stack
just infra-down      # stop everything
```

Or manually:

```bash
cd infra && docker compose up -d --build
```

Endpoints:
- Go API: http://localhost:8080/healthz
- Rust API: http://localhost:8081/healthz
- Jaeger: http://localhost:16686
- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000 (admin/admin)

### Messaging broker

NATS JetStream is the default. To run the Rust backend against Redpanda (a
Kafka-API broker; the same path works with Apache Kafka) instead:

```bash
cd infra && docker compose --profile redpanda up -d --build   # app on :8082
```

This builds the image with `--features redpanda` and sets `EVENT_BROKER=redpanda`.
Rationale and the NATS-vs-Redpanda comparison: [ADR 0001](docs/adr/0001-messaging-broker.md)
and [docs/benchmarks.md](docs/benchmarks.md).

## Configuration

Both backends are configured by environment variables. Defaults are tuned for the
local stack; see `infra/.env.example`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | (required) | Postgres connection string |
| `PORT` | `8080` (Go) / `8081` (Rust) | HTTP listen port |
| `EVENT_BROKER` | `nats` | `nats` or `redpanda` (Rust) |
| `NATS_URL` | (unset = messaging off) | NATS connection; carries user/pass and `tls://` |
| `NATS_CREDS` | (unset) | Path to a NATS JWT/nkey credentials file |
| `REDPANDA_BROKERS` | (unset) | Kafka-API broker list when `EVENT_BROKER=redpanda` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | (unset = tracing off) | OTLP collector endpoint |
| `ADMIN_KEY` | (unset = admin disabled) | Guards operator/admin endpoints (`X-Admin-Key`) |
| `APP_ENV` | (unset) | `production` rejects a weak/unset `ADMIN_KEY` at startup |
| `RATE_LIMIT_RPS` | `50` | Per-client rate on `POST /v1/transfers`; `0` disables |
| `TRUST_PROXY_HEADERS` | `false` | Honor `X-Forwarded-For`/`X-Real-IP` for the rate-limit key (set only behind a trusted proxy) |
| `CORS_ALLOW_ORIGINS` | localhost dev origins | Comma-separated allowed origins |

Operator mutations (zone status/controls, spool replay, incident actions) and
snapshot/restore require `X-Admin-Key`; the audit actor is taken from the
`X-Actor` header, not the request body. See [SECURITY.md](SECURITY.md) for the
production hardening checklist.

## Task runner

This project uses [`just`](https://github.com/casey/just) as the polyglot task runner.

```bash
brew install just    # macOS
just                 # list all recipes
just test            # run all tests (Go + Rust + web build)
just lint            # lint Go + Rust
just infra-up        # start Docker Compose dev stack
just dev-web         # start Vite dev server
```

## API examples

```bash
# List zones
curl -s http://localhost:8080/v1/zones | jq .

# Create a transfer (idempotent via request_id)
curl -s -X POST http://localhost:8080/v1/transfers \
  -H 'content-type: application/json' \
  -d '{"request_id":"req-0001","from_account":"acct-a","to_account":"acct-b","amount_units":120,"zone_id":"zone-eu","metadata":{"note":"demo"}}' | jq .

# Mark a zone DOWN (creates audit log + incident).
# Operator mutations require X-Admin-Key; the audit actor comes from X-Actor.
curl -s -X POST http://localhost:8080/v1/zones/zone-eu/status \
  -H 'content-type: application/json' \
  -H 'X-Admin-Key: dev-admin-key' -H 'X-Actor: operator@example' \
  -d '{"status":"DOWN","reason":"simulated outage"}' | jq .

# Get zone controls
curl -s http://localhost:8080/v1/zones/zone-eu/controls | jq .

# Get spool stats
curl -s http://localhost:8080/v1/zones/zone-eu/spool | jq .

# List audit trail for a zone
curl -s http://localhost:8080/v1/zones/zone-eu/audit | jq .
```

Full API specification: `api/openapi.yaml`

## Testing

Unit tests cover hashing cross-language parity, error handling, canonicalization, and ledger invariants. Contract tests (Schemathesis) validate both backends against the OpenAPI spec in CI.

```bash
just test            # unit tests (Go + Rust)
just lint            # clippy + go vet
```

## Documentation

- [Architecture](docs/architecture.md) - components, the event pipeline, and reconciliation
- [Go vs Rust parity](docs/parity-matrix.md)
- [Event schema](docs/event-schema.md) - envelope and versioning policy
- [Messaging ADR](docs/adr/0001-messaging-broker.md) and [benchmarks](docs/benchmarks.md)
- [Backup and disaster recovery](docs/backup-dr.md)
- [Threat model](docs/threat-model.md)
- [Contributing](CONTRIBUTING.md) - setup, tasks, conventions
- [Security policy](SECURITY.md)

## Security

CI includes CodeQL, govulncheck, cargo-audit, npm audit, and Schemathesis contract tests. See [SECURITY.md](SECURITY.md), `docs/security-scanning.md`, and `docs/threat-model.md`.

## Disclaimer

This project is provided **as-is** for educational, demonstration, and simulation purposes only. It is not financial software and must not be used for real financial transactions, real currency management, or any production financial operations.

**No warranty.** The authors and contributors make no warranties, express or implied, regarding the fitness of this software for any particular purpose, its correctness, reliability, or security.

**No advice.** Nothing in this repository constitutes financial, legal, investment, or professional advice of any kind.

**Your responsibility.** By cloning, forking, copying, modifying, or using this software in any way, you accept full responsibility for any consequences. The repository owners and contributors are not liable for any damages, losses, or issues arising from the use of this code.

**License.** See the [LICENSE](LICENSE) file for the full license terms.
