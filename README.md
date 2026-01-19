# Time Ledger Sim MVP (Go + Rust)

A small but production-flavored simulation backend for a "time-currency" ledger with:
- Double-entry ledger (postings), integer units (recommended: **1 unit = 1 second** for MVP)
- 10 worldwide zones with operator-controlled status (`OK/DEGRADED/DOWN`)
- Fraud/ops incidents per zone (clickable details)
- **At-least-once** messaging with NATS JetStream + **Transactional Outbox** + **Inbox dedup**
- Observability: structured logs, Prometheus metrics, OpenTelemetry traces (Jaeger)

Two interchangeable backends are included:
- `go/` (Go 1.25+) - recommended as the “primary” MVP
- `rust/` (Rust stable) - feature-parity implementation

## Dashboard (web/)

A static operator console is included under `web/`:

```bash
cd web
npm install
npm run dev
```

Open: http://localhost:5173

The Vite dev server proxies `/v1/*` to `http://localhost:8080` by default.

For GitHub Pages deployment, see `.github/workflows/pages.yml`.

## Quickstart (dev stack)

Note: dependency downloads (Go modules / Rust crates) require internet access on your machine.

Requirements:
- Docker + Docker Compose
- Go 1.25+ (for local dev) and/or Rust stable

Tip: for fully reproducible builds, generate lockfiles once and commit them:

```bash
./scripts/bootstrap-lockfiles.sh
```

Start infra + Go service:

```bash
cd infra
docker compose up -d --build
```

The Go API:
- http://localhost:8080/healthz
- http://localhost:8080/metrics

Observability:
- Jaeger: http://localhost:16686
- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000 (admin/admin)

## Example calls

List zones:
```bash
curl -s http://localhost:8080/v1/zones | jq .
```

Create a transfer (idempotent via request_id):
```bash
curl -s -X POST http://localhost:8080/v1/transfers \
  -H 'content-type: application/json' \
  -d '{
    "request_id":"req-0001",
    "from_account":"acct-a",
    "to_account":"acct-b",
    "amount_units":120,
    "zone_id":"zone-eu",
    "metadata":{"note":"demo"}
  }' | jq .
```

Mark a zone DOWN (creates audit log + incident):
```bash
curl -s -X POST http://localhost:8080/v1/zones/zone-eu/status \
  -H 'content-type: application/json' \
  -d '{"status":"DOWN","actor":"operator@example","reason":"simulated outage"}' | jq .
```

## Tests & coverage

### Go
```bash
cd go
go test ./... -coverprofile=cover.out
go tool cover -func=cover.out
```

### Rust
```bash
cd rust/sim
cargo test
```

(Repo includes unit tests for hashing, idempotency conflict handling, and ledger invariants.)

## Security notes (high level)
See `docs/threat-model.md`.


## Additional API

List balances:
```bash
curl -s http://localhost:8080/v1/balances | jq .
```

List transactions:
```bash
curl -s http://localhost:8080/v1/transactions | jq .
```

Get transaction detail (includes postings + metadata):
```bash
curl -s http://localhost:8080/v1/transactions/<txn-id> | jq .
```



## Version manifest

Both backends expose build info at:

- `GET /v1/version`

The dashboard shows this in the header so operators can see which build they’re using.

## Security gates

CI includes CodeQL + govulncheck + cargo-audit + npm audit. See `docs/security-scanning.md`.
