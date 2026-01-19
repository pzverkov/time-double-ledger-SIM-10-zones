# Lightweight Threat Model (MVP)

## Assets (what we protect)
- **Ledger integrity**: transactions, postings, balances (time-currency units)
- **Idempotency correctness**: request_id uniqueness and payload consistency
- **Incident/audit trail integrity**: operator actions and incident records
- **Messaging integrity**: outbox/inbox tables; event streams (JetStream)
- **Availability**: ability for operators to observe and control the simulation
- **Secrets**: DB creds, NATS creds, admin key (snapshot/restore)

## Attacker goals
- Forge or mutate ledger history (create/modify transfers)
- Double-spend via replay / duplicated messages
- Cause denial of service (event storms, huge payloads, slow queries)
- Escalate privileges (operator actions, snapshot restore)
- Exfiltrate data (metadata, audit log, operator identifiers)

## Entry points
- Public HTTP API endpoints (transfers, zone status, snapshot/restore)
- NATS JetStream subjects (`events.*`) (publisher/consumer)
- Postgres (direct access if network exposed)
- Observability endpoints (Prometheus `/metrics`, OTEL exporter, Grafana)

## Mitigations in this repo
- **Immutability**: transactions/postings are insert-only (no updates)
- **Idempotency key + payload hash**: same request_id with different body => **409 conflict**
- **Transactional Outbox**: avoids "DB commit but event missing"
- **Inbox dedup (consumer,event_id PK)**: makes consumers safe under at-least-once
- **Message size limits** (recommended): enforce in API and NATS
- **Validation**: amount_units > 0, known zone, zone DOWN blocks transfers
- **Least privilege** (recommended): separate DB users for app vs migrator
- **Admin-only snapshot/restore**: guarded by `X-Admin-Key` (dev-only)
- **Structured logs** with redaction hooks (do not log full metadata by default)
- **Observability**: metrics + traces for anomaly detection

## PII handling (call-out)
This MVP **does not require PII**. However, PII can sneak in via:
- `metadata` on transfers (free-form JSON)
- `actor` on operator actions

Guidance:
- Treat `metadata` as **untrusted**: avoid storing names/emails; prefer opaque ids.
- In production: add **allowlist validation** for metadata keys, size caps, and retention policies.
- Avoid logging metadata verbatim; logs are high-risk for accidental PII leakage.
- If real PII is ever stored: encrypt at rest where appropriate, restrict access, and define deletion/retention.

## Recommended next hardening steps
- AuthN/AuthZ (JWT / mTLS), role-based operator permissions
- Rate limiting + request body size caps
- Network policies: DB not publicly reachable; NATS protected
- WAF and input sanitization hardening
- Secrets manager (not env files), rotation
- Integrity checks on snapshots + signed exports (optional)
