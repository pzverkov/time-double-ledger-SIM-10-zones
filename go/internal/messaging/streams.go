package messaging

import (
	"context"
	"time"

	"github.com/nats-io/nats.go"
)

const (
	StreamName = "EVENTS"
	// EventSchemaVersion is stamped on every published event; bump on a breaking
	// envelope change. Consumers reject versions they do not support.
	EventSchemaVersion = 1
)

func EnsureStreams(ctx context.Context, js nats.JetStreamContext) error {
	// Create stream if missing (idempotent)
	_, err := js.StreamInfo(StreamName)
	if err == nil {
		return nil
	}
	_, err = js.AddStream(&nats.StreamConfig{
		Name:              StreamName,
		Subjects:          []string{"events.>"},
		Storage:           nats.FileStorage,
		Retention:         nats.LimitsPolicy,
		MaxMsgsPerSubject: 1000000,
		Discard:           nats.DiscardOld,
		Duplicates:        2 * time.Minute, // 2 minutes
	})
	return err
}
