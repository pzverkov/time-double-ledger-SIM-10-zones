# Architecture

A finance-flavored simulation: a double-entry "time-currency" ledger across 10
zones, with operator controls, incident ops, and an event pipeline. Two
interchangeable backends (Go and Rust) implement the same API; a React dashboard
drives them.

## Components

```
                +------------------+
                |  Web dashboard   |   React + Vite (web/)
                |  (operator UI)   |
                +---------+--------+
                          | HTTP (VITE_API_BASE, CORS allowlist)
                          v
        +-----------------+------------------+
        |   API backend (Go OR Rust)         |   go/  |  rust/sim/
        |   axum / chi, same OpenAPI contract|
        +--------+-----------------+---------+
                 | SQL             | publish (transactional outbox)
                 v                 v
          +------------+    +--------------+
          | Postgres   |    | Broker       |  NATS JetStream (default)
          | (ledger,   |    |              |  or Redpanda (Kafka API)
          |  outbox,   |    +------+-------+
          |  inbox)    |           | consume (per-group)
          +-----+------+           v
                |          +-------+--------+--------------+
                |          | fraud-v1       | analytics-v1 |
                |          | (incidents)    | (zone stats) |
                +----------+----------------+--------------+
                           inbox dedup (idempotent)
```

Observability: each backend emits structured JSON logs, Prometheus metrics
(`/metrics`), and OpenTelemetry traces over OTLP to Jaeger. The transfer's W3C
trace context is carried through the outbox and broker into the fraud/analytics
consumers, so a transfer and the work it triggers form a single end-to-end trace.
Compose also runs Prometheus and Grafana.

Continuous reconciliation (Rust backend): a periodic job re-derives each account's
balance from the immutable postings and compares it to the materialized
`balances` row, and counts any transaction whose postings do not net to zero. Both
counts are exposed as gauges (`ledger_balance_drift_accounts`,
`ledger_unbalanced_transactions`) and should always be 0; a nonzero value logs a
warning and is alertable.

## Event pipeline (the core design)

1. A transfer writes the ledger rows and an `outbox_events` row in one DB
   transaction (transactional outbox - no dual-write inconsistency).
2. An outbox publisher relays unpublished rows to the broker, carrying the outbox
   id as the dedup key.
3. Two independent consumer groups receive every event (fan-out):
   - `fraud-v1` flags large transfers into `incidents`.
   - `analytics-v1` folds per-zone aggregates into `zone_event_stats`.
4. Each consumer claims the event in `inbox_events` (keyed `(consumer, event_id)`)
   in the same transaction as its side effect, so redelivery is idempotent.

Delivery is at-least-once: handlers ack on success and leave a message for bounded
redelivery on error, dropping a poison message (with a log) after `MAX_DELIVER`.
Idempotency lives in the inbox table, not the broker, which is why the broker is
swappable. See [ADR 0001](adr/0001-messaging-broker.md).

## Code layout

- `go/internal/` - `app`, `web` (handlers), `ledger`, `messaging`, `util`.
- `rust/sim/src/` - `handlers/`, `messaging/` (`broker`, `store`, `nats`,
  `redpanda`, `fraud`, `analytics`, `outbox`), `state`, `error`, `config`,
  `middleware`, `util`.
- `db/migrations/` - Flyway `V<n>__*.sql`.
- `web/src/` - dashboard (components, hooks, `lib/api`).
- `api/openapi.yaml` - the API contract both backends satisfy.

See [parity-matrix.md](parity-matrix.md) for feature parity between the backends.
