package web

import (
	"net/http/httptest"
	"testing"
)

func TestActorFromRequest(t *testing.T) {
	cases := []struct {
		header string
		want   string
	}{
		{"", "operator"},
		{"   ", "operator"},
		{"alice", "alice"},
		{"  bob  ", "bob"},
	}
	for _, c := range cases {
		r := httptest.NewRequest("POST", "/", nil)
		if c.header != "" {
			r.Header.Set("X-Actor", c.header)
		}
		if got := actorFromRequest(r); got != c.want {
			t.Fatalf("actorFromRequest(%q) = %q, want %q", c.header, got, c.want)
		}
	}
}
