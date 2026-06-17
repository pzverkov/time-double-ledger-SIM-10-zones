# Event schema

Events are published to the `events.transfer_posted` subject (JetStream stream
`EVENTS`) by the transactional outbox and consumed by the fraud and analytics
consumer groups.

## Envelope

`TransferPosted` (the only event today):

| Field            | Type   | Notes                                              |
| ---------------- | ------ | -------------------------------------------------- |
| `event_id`       | string | Outbox row id; the `Nats-Msg-Id` for JetStream de-dup and inbox dedup. |
| `schema_version` | int    | Envelope version (see below).                      |
| `type`           | string | `TransferPosted` (Rust publisher; informational).  |
| `transaction_id` | string | The posted transaction.                            |
| `request_id`     | string | Caller idempotency key (Rust publisher).           |
| `zone_id`        | string | Originating zone.                                  |
| `amount_units`   | int64  | Transfer amount.                                   |
| `created_at`     | string | RFC3339 timestamp.                                 |

## Versioning policy

- `schema_version` is stamped on every published event (Rust:
  `config::EVENT_SCHEMA_VERSION`; Go: `messaging.EventSchemaVersion`), currently `1`.
- Additive, backward-compatible changes (new optional fields) keep the same
  version; consumers parse leniently and ignore unknown fields.
- A breaking change (renamed/removed field, changed meaning) bumps the version.
- Consumers reject an event whose version they do not support rather than
  mis-parsing it: the Rust consumers return an error so the message follows the
  dead-letter path, and the Go consumer logs and skips (no DLQ on that consumer).
- A missing `schema_version` is treated as the current version, so events that
  were already in flight when versioning was introduced still process.
