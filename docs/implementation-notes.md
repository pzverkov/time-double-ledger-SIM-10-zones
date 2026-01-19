# Implementation Notes

## Messaging semantics
The Go implementation includes:
- Transactional Outbox (DB table `outbox_events`)
- Outbox publisher -> NATS JetStream (subject `events.transfer_posted`)
- Fraud consumer (pull) with inbox dedup (`inbox_events`)

The Rust implementation currently focuses on API parity + DB correctness.
Porting the outbox publisher + fraud consumer to Rust is straightforward using `async-nats` JetStream:
- Poll `outbox_events WHERE published_at IS NULL`
- Publish with `Nats-Msg-Id` header = outbox id
- Mark published
- Pull-consume and insert `inbox_events(consumer,event_id)` to dedup

This file exists so the repo stays honest about current parity.
