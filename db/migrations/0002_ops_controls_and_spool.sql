-- Ops controls + spooling for Time Ledger Sim

-- Per-zone controls that operators can toggle to contain blast radius.
CREATE TABLE IF NOT EXISTS zone_controls (
  zone_id TEXT PRIMARY KEY REFERENCES zones(id) ON DELETE CASCADE,
  writes_blocked BOOLEAN NOT NULL DEFAULT FALSE,
  cross_zone_throttle INTEGER NOT NULL DEFAULT 100 CHECK (cross_zone_throttle >= 0 AND cross_zone_throttle <= 100),
  spool_enabled BOOLEAN NOT NULL DEFAULT FALSE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Transfers that are queued when a zone is blocked / down, to be replayed later.
CREATE TABLE IF NOT EXISTS spooled_transfers (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  request_id TEXT NOT NULL UNIQUE,
  payload_hash TEXT NOT NULL,
  from_account TEXT NOT NULL,
  to_account TEXT NOT NULL,
  amount_units BIGINT NOT NULL CHECK (amount_units > 0),
  zone_id TEXT NOT NULL REFERENCES zones(id) ON DELETE CASCADE,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING','APPLIED','FAILED')),
  fail_reason TEXT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  applied_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_spool_zone_status ON spooled_transfers(zone_id, status, created_at);

-- Seed controls for existing zones.
INSERT INTO zone_controls(zone_id)
SELECT id FROM zones
ON CONFLICT (zone_id) DO NOTHING;
