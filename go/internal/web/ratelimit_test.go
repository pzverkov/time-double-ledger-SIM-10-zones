package web

import (
	"net/http/httptest"
	"testing"
	"time"
)

func TestRateLimiterDisabledAlwaysAllows(t *testing.T) {
	rl := newRateLimiter(0, false)
	now := time.Now()
	for i := 0; i < 1000; i++ {
		if !rl.allow("a", now) {
			t.Fatal("disabled limiter must allow")
		}
	}
}

func TestRateLimiterBurstThenDenyThenRefill(t *testing.T) {
	rl := newRateLimiter(5, false) // rps=5, burst=5
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
	rl := newRateLimiter(1, false)
	now := time.Now()
	if !rl.allow("ip1", now) || rl.allow("ip1", now) {
		t.Fatal("ip1 should allow once then deny")
	}
	if !rl.allow("ip2", now) {
		t.Fatal("ip2 has its own bucket")
	}
}

func TestClientKeyIgnoresXFFUnlessTrusted(t *testing.T) {
	r := httptest.NewRequest("POST", "/", nil)
	r.RemoteAddr = "198.51.100.9:5000"
	r.Header.Set("X-Forwarded-For", "203.0.113.7, 10.0.0.1")

	if got := newRateLimiter(1, false).clientKey(r); got != "198.51.100.9" {
		t.Fatalf("untrusted clientKey = %q, want peer 198.51.100.9", got)
	}
	if got := newRateLimiter(1, true).clientKey(r); got != "203.0.113.7" {
		t.Fatalf("trusted clientKey = %q, want 203.0.113.7", got)
	}
}
