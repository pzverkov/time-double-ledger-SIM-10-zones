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

## Results (fill in)

| Metric | NATS JetStream | Redpanda |
| ------ | -------------- | -------- |
| Ingest throughput (req/s) | | |
| End-to-end events/s (drain) | | |
| API p95 latency (ms) | | |
| Peak `outbox_backlog` | | |

Attach a Grafana screenshot of `outbox_backlog` and request rate over the run.

## Notes

- Both brokers carry the same correctness guarantees here: idempotency comes from
  the inbox table, not the broker (see ADR 0001).
- Real-broker delivery semantics (ack-on-success, redelivery-on-error, poison
  bound) are exercised end to end by `scripts/e2e/transfers_e2e.sh`; the in-process
  unit suite covers the failure paths with fakes.
