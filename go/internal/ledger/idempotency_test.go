package ledger

import "testing"

func TestIdempotencyConflictSentinel(t *testing.T) {
  if !IsIdempotencyConflict(ErrIdempotencyConflict) {
    t.Fatalf("expected true")
  }
}
