package ledger

import (
	"errors"
	"fmt"
	"testing"
)

func TestIdempotencyConflictSentinel(t *testing.T) {
	if !IsIdempotencyConflict(ErrIdempotencyConflict) {
		t.Fatal("expected true")
	}
}

func TestIdempotencyConflict_NotOtherErrors(t *testing.T) {
	if IsIdempotencyConflict(ErrZoneDown) {
		t.Fatal("ErrZoneDown should not match idempotency conflict")
	}
	if IsIdempotencyConflict(fmt.Errorf("random error")) {
		t.Fatal("random error should not match idempotency conflict")
	}
}

func TestZoneDownSentinel(t *testing.T) {
	if !IsZoneDown(ErrZoneDown) {
		t.Fatal("expected true")
	}
	if IsZoneDown(ErrIdempotencyConflict) {
		t.Fatal("should not match")
	}
}

func TestZoneBlockedSentinel(t *testing.T) {
	if !IsZoneBlocked(ErrZoneBlocked) {
		t.Fatal("expected true")
	}
	if IsZoneBlocked(ErrZoneDown) {
		t.Fatal("should not match")
	}
}

func TestWrappedErrors(t *testing.T) {
	wrapped := fmt.Errorf("outer: %w", ErrIdempotencyConflict)
	if !errors.Is(wrapped, ErrIdempotencyConflict) {
		t.Fatal("wrapped error should still match via errors.Is")
	}
}
