# Messaging benchmarks

NATS-vs-Redpanda on an identical workload, backing the right-sizing decision in
[ADR 0001](adr/0001-messaging-broker.md). Numbers below are a template; fill them
from a real run.

## Methodology

1. Bring up the stack on one broker:
   - NATS (default): `docker compose -f infra/docker-compose.yml up -d --build`
   - Redpanda: `docker compose -f infra/docker-compose.yml --profile redpanda up -d --build`
     (app on `:8082`, built with `--features redpanda`)
2. API load + thresholds:
   `k6 run -e BASE_URL=http://localhost:8081 scripts/load/transfers.k6.js`
3. End-to-end pipeline throughput + drain:
   `N=5000 CONCURRENCY=50 BASE_URL=http://localhost:8081 COMPOSE_FILE=infra/docker-compose.yml scripts/load/pipeline_lag.sh`
4. SLI during the run: `outbox_backlog` on `/metrics`, scraped by Prometheus,
   visualized in Grafana (`:3000`).
5. Micro-benchmarks (broker-independent hot paths): `cargo bench`.

Record hardware (CPU, cores, RAM) and broker versions with each result set.

## Results

Run: 2026-06-15. Host: Apple M1 Pro, 8 cores, 16 GB, macOS 26.5; Docker 29.5
(Desktop, single-node). Versions: NATS 2.12.5, Redpanda v24.3.1, Postgres 16.13.
Workload: `transfers.k6.js` at 200 req/s for 30s (6000 reqs); `pipeline_lag.sh`
with N=5000, concurrency 50, to `zone-eu`. NATS and Redpanda apps were run one at
a time (they share one Postgres/outbox).

| Metric | NATS JetStream | Redpanda |
| ------ | -------------- | -------- |
| API requests, failed fraction | 6000, 0.0000 | 6001, 0.0000 |
| API latency p95 (ms) | 11.0 | 4.4 |
| Ingest throughput (req/s) | 221 | 225 |
| End-to-end drain (events/s) | 199 | 128 |
| Peak `outbox_backlog` | 530 | 2090 |

## How to read these numbers (caveats matter more than the table)

This is a single-laptop, single-node, dev-container run. It is a relative sanity
check, not a production capacity statement. The bottlenecks are in our pipeline,
not the brokers:

- **End-to-end drain is publisher-bound, not broker-bound.** The outbox publisher
  relays batches of 50 every 250 ms = a ~200 events/s ceiling regardless of
  broker. NATS sits right at that ceiling (199/s). To benchmark the brokers
  themselves, raise the batch size / shorten the interval first.
- **Redpanda drained slower (128/s) for a reason in our code, not the broker:**
  the Redpanda consumer does a synchronous offset commit per message
  (correctness-first); two consumer groups each paying a per-message commit
  round-trip throttles consumption below the publisher ceiling and lets backlog
  build (peak 2090). Batched/periodic commits would close most of this gap. NATS
  pull-acks are also per-message but cheaper here.
- **API latency is not broker-attributable.** `POST /v1/transfers` only writes to
  Postgres + the outbox; publishing is async. The higher NATS API p95 (11 ms vs
  4.4 ms) is mostly just-started-container warmup, not the broker.
- **Ingest req/s (~220) is load-harness-bound.** `pipeline_lag.sh` spawns one
  curl per request under `xargs -P50`; that process overhead caps offered load
  below what the server sustains (k6 held a clean 200 req/s with 0 failures).

Net: at this scale the broker is not the limiting factor, which is the point of
ADR 0001 - NATS is right-sized and the seam keeps Redpanda available for when the
workload actually outgrows it.

Attach a Grafana screenshot of `outbox_backlog` and request rate over the run.

## Notes

- Both brokers carry the same correctness guarantees here: idempotency comes from
  the inbox table, not the broker (see ADR 0001). The e2e check (fraud + analytics
  fan-out, idempotent re-post) passed identically on both.
- Real-broker delivery semantics (ack-on-success, redelivery-on-error, poison
  bound) are exercised end to end by `scripts/e2e/transfers_e2e.sh`; the in-process
  unit suite covers the failure paths with fakes.
