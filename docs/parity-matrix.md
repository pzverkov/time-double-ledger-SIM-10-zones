# Go vs Rust parity matrix

Both backends implement the same HTTP API (validated in CI by Schemathesis against
`api/openapi.yaml`). They diverge only in messaging, where the Rust backend is
currently ahead.

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

## Observability (full parity)

Structured JSON logs, Prometheus metrics, OpenTelemetry traces (OTLP -> Jaeger).
