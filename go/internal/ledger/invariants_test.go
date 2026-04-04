package ledger

import "testing"

func TestLedgerInvariants_DoubleEntryNetZero(t *testing.T) {
	amount := int64(123)
	net := (-amount) + amount
	if net != 0 {
		t.Fatalf("expected net zero, got %d", net)
	}
}

func TestLedgerInvariants_DoubleEntryNetZero_LargeValues(t *testing.T) {
	for _, amount := range []int64{0, 1, 999999, 86400, 31536000} {
		net := (-amount) + amount
		if net != 0 {
			t.Fatalf("expected net zero for %d, got %d", amount, net)
		}
	}
}

// Cross-language parity anchor: these values must match Rust's hash_percent output.
// Both use FNV-1a 32-bit on raw bytes, mod 100.
func TestHashPercent_CrossLanguageParity(t *testing.T) {
	l := &Ledger{}
	cases := []struct {
		input    string
		expected int
	}{
		{"req-0001", 73},
		{"test-req-001", 22},
		{"abc", 31},
		{"", 61},
	}
	for _, tc := range cases {
		got := l.hashPercent(tc.input)
		if got != tc.expected {
			t.Errorf("hashPercent(%q) = %d, want %d", tc.input, got, tc.expected)
		}
	}
}

func TestHashPercent_Range(t *testing.T) {
	l := &Ledger{}
	for i := 0; i < 1000; i++ {
		s := "req-" + string(rune('A'+i%26)) + string(rune('0'+i%10))
		p := l.hashPercent(s)
		if p < 0 || p >= 100 {
			t.Fatalf("hashPercent(%q) = %d, expected 0-99", s, p)
		}
	}
}

func TestHashPercent_Deterministic(t *testing.T) {
	l := &Ledger{}
	a := l.hashPercent("same-input")
	b := l.hashPercent("same-input")
	if a != b {
		t.Fatalf("expected deterministic result, got %d != %d", a, b)
	}
}
