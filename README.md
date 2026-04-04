# Time Ledger Sim (Go + Rust)

A production-flavored simulation backend for a "time-currency" double-entry ledger with:
- Double-entry ledger (postings), integer units (1 unit = 1 second)
- 10 worldwide zones with operator-controlled status (`OK/DEGRADED/DOWN`)
- Zone controls: writes blocking, cross-zone throttle (0-100%), spool-and-replay
- Fraud/ops incidents per zone with ACK/ASSIGN/RESOLVE lifecycle
- Deterministic throttling via FNV-1a hashing (cross-language parity between Go and Rust)
- **At-least-once** messaging with NATS JetStream + **Transactional Outbox** + **Inbox dedup**
- Observability: structured logs, Prometheus metrics, OpenTelemetry traces (Jaeger)

Two interchangeable backends with full feature parity:
- `go/` (Go 1.26+) - primary implementation
- `rust/sim/` (Rust edition 2024) - feature-parity implementation with modular architecture

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

# Mark a zone DOWN (creates audit log + incident)
curl -s -X POST http://localhost:8080/v1/zones/zone-eu/status \
  -H 'content-type: application/json' \
  -d '{"status":"DOWN","actor":"operator@example","reason":"simulated outage"}' | jq .

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

## Security

CI includes CodeQL, govulncheck, cargo-audit, npm audit, and Schemathesis contract tests. See `docs/security-scanning.md` and `docs/threat-model.md`.

## Disclaimer

This project is provided **as-is** for educational, demonstration, and simulation purposes only. It is not financial software and must not be used for real financial transactions, real currency management, or any production financial operations.

**No warranty.** The authors and contributors make no warranties, express or implied, regarding the fitness of this software for any particular purpose, its correctness, reliability, or security.

**No advice.** Nothing in this repository constitutes financial, legal, investment, or professional advice of any kind.

**Your responsibility.** By cloning, forking, copying, modifying, or using this software in any way, you accept full responsibility for any consequences. The repository owners and contributors are not liable for any damages, losses, or issues arising from the use of this code.

**License.** See the [LICENSE](LICENSE) file for the full license terms.
