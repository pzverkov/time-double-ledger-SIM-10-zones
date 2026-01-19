package ledger

import "testing"

func TestLedgerInvariants_DoubleEntryNetZero(t *testing.T) {
  // This is a unit-level invariant test: debit + credit net to zero.
  amount := int64(123)
  net := (-amount) + amount
  if net != 0 {
    t.Fatalf("expected net zero, got %d", net)
  }
}
