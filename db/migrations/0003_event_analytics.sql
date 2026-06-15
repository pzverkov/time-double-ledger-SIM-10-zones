-- Per-zone event aggregates maintained by the analytics consumer group.
-- A second independent consumer on events.transfer_posted (fan-out alongside fraud).
CREATE TABLE IF NOT EXISTS zone_event_stats (
  zone_id TEXT PRIMARY KEY,
  event_count BIGINT NOT NULL DEFAULT 0,
  total_amount_units BIGINT NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
