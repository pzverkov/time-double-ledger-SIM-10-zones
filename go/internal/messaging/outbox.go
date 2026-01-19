package messaging

import (
  "context"
  "encoding/json"
  "time"

  "github.com/jackc/pgx/v5/pgxpool"
  "github.com/nats-io/nats.go"
  "log/slog"
)

type OutboxPublisher struct {
  db *pgxpool.Pool
  js nats.JetStreamContext
  log *slog.Logger
}

func NewOutboxPublisher(db *pgxpool.Pool, js nats.JetStreamContext, log *slog.Logger) *OutboxPublisher {
  return &OutboxPublisher{db: db, js: js, log: log}
}

func (p *OutboxPublisher) Run(ctx context.Context) {
  ticker := time.NewTicker(250 * time.Millisecond)
  defer ticker.Stop()
  for {
    select {
    case <-ctx.Done():
      return
    case <-ticker.C:
      _ = p.publishBatch(ctx, 50)
    }
  }
}

type outboxRow struct {
  ID string
  EventType string
  Payload []byte
}

func (p *OutboxPublisher) publishBatch(ctx context.Context, limit int) error {
  rows, err := p.db.Query(ctx, `
    SELECT id::text, event_type, payload
    FROM outbox_events
    WHERE published_at IS NULL
    ORDER BY created_at
    LIMIT $1
  `, limit)
  if err != nil { return err }
  defer rows.Close()

  batch := []outboxRow{}
  for rows.Next() {
    var r outboxRow
    if err := rows.Scan(&r.ID, &r.EventType, &r.Payload); err != nil { return err }
    batch = append(batch, r)
  }
  if len(batch) == 0 { return nil }

  for _, r := range batch {
    // attach event_id = outbox id if not present
    var m map[string]any
    _ = json.Unmarshal(r.Payload, &m)
    if _, ok := m["event_id"]; !ok || m["event_id"] == "generated_by_db" {
      m["event_id"] = r.ID
    }
    body, _ := json.Marshal(m)

    // NATS message-id enables JetStream de-dup
    msg := &nats.Msg{Subject: "events.transfer_posted", Data: body, Header: nats.Header{}}
    msg.Header.Set("Nats-Msg-Id", r.ID)

    if _, err := p.js.PublishMsg(msg); err != nil {
      p.log.Warn("publish failed", "event_id", r.ID, "err", err.Error())
      return err
    }

    _, err := p.db.Exec(ctx, `UPDATE outbox_events SET published_at=now() WHERE id=$1::uuid`, r.ID)
    if err != nil {
      p.log.Warn("mark published failed", "event_id", r.ID, "err", err.Error())
      return err
    }
  }
  return nil
}
