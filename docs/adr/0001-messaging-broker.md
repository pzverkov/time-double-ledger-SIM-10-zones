# ADR 0001: Messaging broker choice and seam

Status: accepted
Date: 2026-06-15

## Context

The event pipeline is a transactional outbox: the transfer write and an
`outbox_events` row commit in one DB transaction, a publisher relays unpublished
rows to a broker, and consumers act on them (fraud detection, analytics) with
inbox dedup for idempotency. We needed to pick a broker and decide how tightly to
couple to it. The workload is small (a 10-zone simulation), but the project is
also a portfolio piece, so the design must show judgment, not just a running
binary.

## Options considered

- **Postgres as the queue** (`SELECT ... FOR UPDATE SKIP LOCKED`). Zero new
  infrastructure; we already run Postgres and own the outbox/inbox tables.
  Handles far more throughput than this sim produces. Weaker at many independent
  consumer groups and log replay.
- **NATS JetStream**. Single ~15 MB Go binary, no JVM, durable streams,
  publish-side dedup via `Nats-Msg-Id`, pull consumers with `max_deliver`. Light
  to operate. Not built for huge multi-consumer-group analytics or long retention.
- **Apache Kafka**. The throughput/replay standard, but JVM + heavier ops
  (KRaft/ZooKeeper). Overkill here.
- **Redpanda**. Kafka API, single C++ binary, no JVM/ZooKeeper. The pragmatic
  "Kafka when you actually need Kafka" choice, and the easiest Kafka-family option
  to run in compose.

## Decision

Default to **NATS JetStream**. It is right-sized for this workload: durable,
idempotent at-least-once delivery with the least operational weight. Kafka's
strengths (massive multi-group throughput, long retention, replay) are not
exercised by a 10-zone sim, so adopting it would be over-engineering.

To keep the choice from being a one-way door, the broker sits behind a seam:

- `EventPublisher` / `EventConsumer` / `EventHandler` traits (`messaging/broker.rs`).
- DB access behind `OutboxStore` / `FraudStore` / `AnalyticsStore` ports
  (`messaging/store.rs`), so business logic is broker- and DB-agnostic and unit
  testable with fakes.
- A real, feature-gated Kafka-API implementation (`messaging/redpanda.rs`,
  `--features redpanda`) proves the seam is real, not theoretical, and backs the
  benchmark.

`EVENT_BROKER=nats|redpanda` selects the implementation at startup.

## Invariants

- **Idempotency lives in the inbox table, not the broker.** NATS provides
  publish dedup (`Nats-Msg-Id`); Kafka does not. Correctness therefore relies on
  `inbox_events` (keyed `(consumer, event_id)`), where each handler's claim and
  its side effect commit in one transaction. This is exactly why the pattern
  ports cleanly across brokers.
- **Failure is never silent.** A handler error leaves the message un-acked for
  redelivery, bounded by `max_deliver` (NATS) / a retry budget (Kafka); on
  exhaustion the message is dropped with a logged warning.

## When to revisit

Switch the default to Redpanda when the simulation grows N independent consumer
groups that each replay full history, needs long retention, or needs stream
processing. Until then, NATS is the correct default and the seam makes the switch
cheap. See `docs/benchmarks.md` for the NATS-vs-Redpanda comparison.
