-- Carry the originating request's W3C trace context through the outbox so the
-- fraud/analytics consumers can link their spans to the transfer's trace.
ALTER TABLE outbox_events ADD COLUMN traceparent TEXT NULL;
