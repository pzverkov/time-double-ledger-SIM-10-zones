# Implementation Notes

## Messaging semantics

Transactional outbox pipeline, broker-agnostic behind a trait seam:

- Transactional Outbox (DB table `outbox_events`): the transfer write and the
  event row commit together.
- `OutboxPublisher` relays unpublished rows to an `EventPublisher`, carrying the
  outbox id as the dedup key.
- Two independent consumer groups (fan-out), each an `EventHandler` driven by an
  `EventConsumer`:
  - `fraud-v1` flags large transfers into `incidents`.
  - `analytics-v1` folds per-zone aggregates into `zone_event_stats`.
- Inbox dedup (`inbox_events`, keyed `(consumer, event_id)`): each handler's
  claim and its side effect commit in one DB transaction, so a redelivery
  reprocesses cleanly and never double-writes.

### Seam (Rust)

- `messaging/broker.rs`: `EventPublisher`, `EventConsumer`, `EventHandler`,
  `IncomingEvent`.
- `messaging/store.rs`: `OutboxStore` / `FraudStore` / `AnalyticsStore` ports +
  `PgStore` impl + pure `fraud_verdict` / `fold_stats`. Ports make handlers and
  the publisher unit-testable with fakes (dedup, error-path, partial-failure).
- `messaging/nats.rs`: NATS JetStream impl (default).
- `messaging/redpanda.rs`: Kafka-API impl behind `--features redpanda`.
- `EVENT_BROKER=nats|redpanda` selects the impl at startup. NATS is the default;
  see [ADR 0001](adr/0001-messaging-broker.md).

### Delivery guarantees

At-least-once. Handler `Ok` acks (NATS) / commits offset (Kafka); handler `Err`
leaves the message for redelivery, bounded by `max_deliver` (NATS) / a retry
budget (Kafka). On exhaustion the message is dropped with a logged warning, never
silently. Idempotency comes from the inbox table, not the broker (NATS has
`Nats-Msg-Id`; Kafka does not).

### Testing

- Unit + in-memory broker: `cargo test` (failure paths, dedup, fan-out, partial
  publish failure, redelivery).
- End-to-end against a live stack, either broker: `scripts/e2e/transfers_e2e.sh`.
- Load + throughput: `scripts/load/transfers.k6.js`, `scripts/load/pipeline_lag.sh`.
- Micro-benchmarks: `cargo bench`. Comparison writeup: `docs/benchmarks.md`.

## Go vs Rust parity

The Go implementation includes the same outbox publisher + fraud consumer with
inbox dedup. The Rust implementation adds the broker seam, the analytics consumer
group, and the feature-gated Redpanda path.
