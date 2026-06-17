# Go vs Rust parity matrix

Both backends implement the same HTTP API (validated in CI by Schemathesis against
`api/openapi.yaml`). They diverge in messaging and in a few periodic ops jobs,
where the Rust backend is currently ahead.

## HTTP API (full parity)

| Endpoint | Go | Rust |
| --- | --- | --- |
| `GET /healthz`, `GET /metrics`, `GET /v1/version` | yes | yes |
| `GET /v1/zones`, `POST /v1/zones/{id}/status` | yes | yes |
| `POST /v1/transfers` | yes | yes |
| `GET /v1/balances` | yes | yes |
| `GET /v1/transactions`, `GET /v1/transactions/{id}` | yes | yes |
| `GET /v1/zones/{id}/incidents`, `GET /v1/incidents`, `GET /v1/incidents/{id}`, `POST /v1/incidents/{id}/action` | yes | yes |
| `GET/POST /v1/zones/{id}/controls` | yes | yes |
| `GET /v1/zones/{id}/spool`, `POST /v1/zones/{id}/spool/replay` | yes | yes |
| `GET /v1/zones/{id}/audit` | yes | yes |
| `POST /v1/sim/snapshot`, `POST /v1/sim/restore` (admin) | yes | yes |

## Ledger and ops (full parity)

| Capability | Go | Rust |
| --- | --- | --- |
| Double-entry postings, idempotency, inbox/outbox | yes | yes |
| Zone controls (block, throttle, spool) | yes | yes |
| Deterministic FNV-1a throttle (cross-language parity test) | yes | yes |
| Snapshot/restore | yes | yes |

## Messaging (Rust ahead)

| Capability | Go | Rust |
| --- | --- | --- |
| Transactional outbox publisher | yes | yes |
| Fraud consumer with inbox dedup | yes | yes |
| Analytics consumer group (`zone_event_stats`, fan-out) | no | yes |
| Swappable broker behind traits | no | yes (`EventPublisher`/`EventConsumer`/`EventHandler`) |
| NATS JetStream | yes | yes (default) |
| Redpanda / Kafka API | no | yes (`--features redpanda`) |
| Explicit ack-on-success, bounded redelivery, poison drop | partial | yes |
| Trace context propagated across the broker | no | yes |
| Dead-letter queue for poison events | no | yes |
| Event `schema_version` stamped + unknown version rejected | yes | yes |

## Security and resilience

| Capability | Go | Rust |
| --- | --- | --- |
| Admin-key guard on operator mutations, audit actor from `X-Actor` | yes | yes |
| Per-client rate limit on `POST /v1/transfers` (peer-IP keyed) | yes | yes |
| Production weak/unset `ADMIN_KEY` startup guard (`APP_ENV`) | yes | yes |
| NATS auth via `NATS_CREDS` / authenticated URL | yes | yes |
| Bounded, recycling DB connection pool | yes | yes |
| Outbox/inbox/DLQ retention job | no | yes |
| Continuous ledger reconciliation (balance/posting invariants) | no | yes |

## Observability (full parity)

Structured JSON logs, Prometheus metrics, OpenTelemetry traces (OTLP -> Jaeger).
The Rust backend additionally propagates the W3C trace context through the outbox
and broker, so transfer and consumer spans share one trace, and exposes ledger
reconciliation drift gauges.
