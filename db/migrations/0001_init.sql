-- Time Ledger Sim MVP schema (Postgres)

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Zones
CREATE TABLE IF NOT EXISTS zones (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('OK','DEGRADED','DOWN')),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Accounts (zone-scoped for simulation)
CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  zone_id TEXT NOT NULL REFERENCES zones(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Transactions (immutable)
CREATE TABLE IF NOT EXISTS transactions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  request_id TEXT NOT NULL UNIQUE,
  payload_hash TEXT NOT NULL,
  from_account TEXT NOT NULL,
  to_account TEXT NOT NULL,
  amount_units BIGINT NOT NULL CHECK (amount_units > 0),
  zone_id TEXT NOT NULL REFERENCES zones(id),
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Double-entry postings (immutable)
CREATE TABLE IF NOT EXISTS postings (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  txn_id UUID NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id),
  direction TEXT NOT NULL CHECK (direction IN ('DEBIT','CREDIT')),
  amount_units BIGINT NOT NULL CHECK (amount_units > 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Balance projection (fast reads)
CREATE TABLE IF NOT EXISTS balances (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  balance_units BIGINT NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Audit log for operator actions
CREATE TABLE IF NOT EXISTS audit_log (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  reason TEXT NULL,
  details JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Incidents (fraud/ops)
CREATE TABLE IF NOT EXISTS incidents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  zone_id TEXT NOT NULL REFERENCES zones(id),
  related_txn_id UUID NULL REFERENCES transactions(id),
  severity TEXT NOT NULL CHECK (severity IN ('INFO','WARN','CRITICAL')),
  status TEXT NOT NULL CHECK (status IN ('OPEN','ACK','RESOLVED')) DEFAULT 'OPEN',
  title TEXT NOT NULL,
  details JSONB NOT NULL DEFAULT '{}'::jsonb,
  detected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_incidents_zone_time ON incidents(zone_id, detected_at DESC);

-- Transactional Outbox
CREATE TABLE IF NOT EXISTS outbox_events (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_type TEXT NOT NULL,
  aggregate_type TEXT NOT NULL,
  aggregate_id TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  published_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_outbox_unpublished ON outbox_events(published_at, created_at);

-- Inbox (consumer-side dedup for at-least-once)
CREATE TABLE IF NOT EXISTS inbox_events (
  consumer TEXT NOT NULL,
  event_id UUID NOT NULL,
  processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (consumer, event_id)
);

-- Seed 10 zones (id values are stable for demos)
INSERT INTO zones (id, name, status) VALUES
  ('zone-na', 'North America', 'OK'),
  ('zone-sa', 'South America', 'OK'),
  ('zone-eu', 'Europe', 'OK'),
  ('zone-uk', 'United Kingdom', 'OK'),
  ('zone-af', 'Africa', 'OK'),
  ('zone-me', 'Middle East', 'OK'),
  ('zone-in', 'India', 'OK'),
  ('zone-cn', 'China', 'OK'),
  ('zone-ap', 'Asia Pacific', 'OK'),
  ('zone-au', 'Australia', 'OK')
ON CONFLICT (id) DO NOTHING;
