# Backup and disaster recovery

This is a demo, so no backups run by default. This documents the policy a real
deployment would follow, and the connection-pool resilience already in the code.

## What must be durable

Postgres is the system of record: `transactions`, `postings`, `balances`,
`zones`, `zone_controls`, `incidents`, `audit_log`, and the messaging tables
(`outbox_events`, `inbox_events`, `dead_letter_events`). NATS/Redpanda hold only
in-flight events; the transactional outbox means a lost broker is re-driven from
Postgres, so the broker is not a backup target. Rebuildable derived state
(`balances`, analytics aggregates) is still backed up to avoid a slow replay, and
the reconciliation job (see architecture.md) detects drift after a restore.

## Backup strategy

- **Continuous archiving + PITR**: stream WAL to object storage (e.g. `pgBackRest`
  or a managed provider) with a base backup at least daily. This gives
  point-in-time recovery to any moment within the retention window.
- **Logical dumps**: a daily `pg_dump` retained off-site as a second, format-
  independent copy and for partial restores.
- Encrypt backups at rest and in transit; restrict access to the backup bucket.
- Periodically test-restore into a scratch database; an untested backup is not a
  backup.

## Targets

- **RPO** (max data loss): <= 5 minutes, bounded by WAL archive frequency.
- **RTO** (max downtime): <= 1 hour to restore the latest base backup and replay
  WAL on a fresh instance.

## Restore procedure (outline)

1. Provision a new Postgres instance.
2. Restore the latest base backup, then replay WAL to the target time (PITR).
3. Run `flyway migrate` to confirm the schema is at the expected version.
4. Point the backends at the restored instance (`DATABASE_URL`); they reconnect
   via the pool's verified recycling.
5. Confirm `/readyz` is healthy and `ledger_balance_drift_accounts` /
   `ledger_unbalanced_transactions` are 0.

## Connection-pool resilience (implemented)

Both backends bound and recycle the pool so it survives a DB restart/failover
without serving dead connections:

- **Rust** (`deadpool-postgres`): `Verified` recycling re-checks a connection
  before it is handed out; `max_size=16`. Per-connection `statement_timeout` and
  `idle_in_transaction_session_timeout` bound runaway queries.
- **Go** (`pgxpool`): `MaxConns=16`, `MinConns=2`, `MaxConnLifetime=30m`,
  `MaxConnIdleTime=5m`, `HealthCheckPeriod=1m`, so aged and stale connections are
  retired and pruned before a request gets one.

A failed connection acquisition surfaces as a request error within the request
timeout rather than hanging, and `/readyz` fails so the instance is pulled from
rotation.
