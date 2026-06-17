package web

import (
	"net/http/httptest"
	"testing"
	"time"
)

func TestRateLimiterDisabledAlwaysAllows(t *testing.T) {
	rl := newRateLimiter(0)
	now := time.Now()
	for i := 0; i < 1000; i++ {
		if !rl.allow("a", now) {
			t.Fatal("disabled limiter must allow")
		}
	}
}

func TestRateLimiterBurstThenDenyThenRefill(t *testing.T) {
	rl := newRateLimiter(5) // rps=5, burst=5
	t0 := time.Now()
	for i := 0; i < 5; i++ {
		if !rl.allow("ip1", t0) {
			t.Fatalf("request %d in burst should be allowed", i)
		}
	}
	if rl.allow("ip1", t0) {
		t.Fatal("6th request should be denied")
	}
	t1 := t0.Add(time.Second)
	for i := 0; i < 5; i++ {
		if !rl.allow("ip1", t1) {
			t.Fatalf("post-refill request %d should be allowed", i)
		}
	}
	if rl.allow("ip1", t1) {
		t.Fatal("should be denied again after burst consumed")
	}
}

func TestRateLimiterPerKey(t *testing.T) {
	rl := newRateLimiter(1)
	now := time.Now()
	if !rl.allow("ip1", now) || rl.allow("ip1", now) {
		t.Fatal("ip1 should allow once then deny")
	}
	if !rl.allow("ip2", now) {
		t.Fatal("ip2 has its own bucket")
	}
}

func TestClientKeyPrefersXFF(t *testing.T) {
	r := httptest.NewRequest("POST", "/", nil)
	r.Header.Set("X-Forwarded-For", "203.0.113.7, 10.0.0.1")
	if got := clientKey(r); got != "203.0.113.7" {
		t.Fatalf("clientKey = %q, want 203.0.113.7", got)
	}
}
