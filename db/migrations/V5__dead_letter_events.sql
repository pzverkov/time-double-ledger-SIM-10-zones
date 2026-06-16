-- Poison events that a consumer could not process after MAX_DELIVER redeliveries.
-- Recorded for inspection/replay instead of being silently dropped.
CREATE TABLE IF NOT EXISTS dead_letter_events (
  id BIGSERIAL PRIMARY KEY,
  consumer TEXT NOT NULL,
  event_id TEXT,
  payload JSONB NOT NULL,
  error TEXT,
  delivered INT NOT NULL,
  dead_lettered_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
