package messaging

import (
  "context"
  "encoding/json"
  "time"

  "github.com/jackc/pgx/v5/pgxpool"
  "github.com/nats-io/nats.go"
  "log/slog"
)

type FraudConsumer struct {
  db *pgxpool.Pool
  js nats.JetStreamContext
  log *slog.Logger
}

func NewFraudConsumer(db *pgxpool.Pool, js nats.JetStreamContext, log *slog.Logger) *FraudConsumer {
  return &FraudConsumer{db: db, js: js, log: log}
}

type transferPosted struct {
  EventID string `json:"event_id"`
  TransactionID string `json:"transaction_id"`
  ZoneID string `json:"zone_id"`
  AmountUnits int64 `json:"amount_units"`
  CreatedAt string `json:"created_at"`
}

func (c *FraudConsumer) Run(ctx context.Context) {
  sub, err := c.js.PullSubscribe("events.transfer_posted", "fraud-v1", nats.BindStream(StreamName))
  if err != nil {
    c.log.Error("fraud subscribe failed", "err", err.Error())
    return
  }

  for {
    select {
    case <-ctx.Done():
      return
    default:
    }

    msgs, err := sub.Fetch(10, nats.MaxWait(1*time.Second))
    if err != nil && err != nats.ErrTimeout {
      c.log.Warn("fetch failed", "err", err.Error())
      continue
    }
    for _, msg := range msgs {
      _ = c.handleMsg(ctx, msg)
    }
  }
}

func (c *FraudConsumer) handleMsg(ctx context.Context, msg *nats.Msg) error {
  var ev transferPosted
  if err := json.Unmarshal(msg.Data, &ev); err != nil {
    _ = msg.Ack()
    return nil
  }
  if ev.EventID == "" {
    // fallback: JetStream msg id header
    ev.EventID = msg.Header.Get("Nats-Msg-Id")
  }
  if ev.EventID == "" {
    _ = msg.Ack()
    return nil
  }

  // inbox dedup
  _, err := c.db.Exec(ctx, `INSERT INTO inbox_events(consumer,event_id) VALUES('fraud-v1',$1::uuid) ON CONFLICT DO NOTHING`, ev.EventID)
  if err != nil {
    c.log.Warn("inbox insert failed", "event_id", ev.EventID, "err", err.Error())
    return err // retry => at-least-once
  }

  // basic fraud rule: unusually large transfer triggers incident
  if ev.AmountUnits >= 3600 { // 1 hour worth (in seconds)
    _, err := c.db.Exec(ctx, `
      INSERT INTO incidents(zone_id, related_txn_id, severity, title, details)
      VALUES($1, $2::uuid, 'WARN', 'Large time transfer', jsonb_build_object('amount_units',$3,'rule','large_transfer'))
    `, ev.ZoneID, ev.TransactionID, ev.AmountUnits)
    if err != nil {
      c.log.Warn("incident insert failed", "event_id", ev.EventID, "err", err.Error())
      return err
    }
  }

  _ = msg.Ack()
  return nil
}
