package web

import (
	"math"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

// Per-client token-bucket rate limiter for the public write path. Keyed by
// client IP (X-Forwarded-For / X-Real-IP, falling back to RemoteAddr). rps
// tokens accrue per second up to a burst ceiling; a request costs one token.
// Disabled when rps == 0 (used by the contract-test stack so fuzzing is not
// throttled). App-level limiting is a backstop; a real edge deployment should
// also rate-limit at the gateway.

const (
	rlGCThreshold = 8192
	rlIdleTTL     = 60 * time.Second
)

type rlBucket struct {
	tokens float64
	last   time.Time
}

type rateLimiter struct {
	rps     float64
	burst   float64
	mu      sync.Mutex
	buckets map[string]*rlBucket
}

func newRateLimiter(rps int) *rateLimiter {
	return &rateLimiter{rps: float64(rps), burst: float64(rps), buckets: map[string]*rlBucket{}}
}

func (rl *rateLimiter) enabled() bool { return rl.rps > 0 }

// allow refills the client's bucket for the elapsed time and consumes one token.
func (rl *rateLimiter) allow(key string, now time.Time) bool {
	if !rl.enabled() {
		return true
	}
	rl.mu.Lock()
	defer rl.mu.Unlock()
	if len(rl.buckets) > rlGCThreshold {
		for k, b := range rl.buckets {
			if now.Sub(b.last) > rlIdleTTL {
				delete(rl.buckets, k)
			}
		}
	}
	b := rl.buckets[key]
	if b == nil {
		b = &rlBucket{tokens: rl.burst, last: now}
		rl.buckets[key] = b
	}
	elapsed := now.Sub(b.last).Seconds()
	b.tokens = math.Min(b.tokens+elapsed*rl.rps, rl.burst)
	b.last = now
	if b.tokens >= 1 {
		b.tokens--
		return true
	}
	return false
}

// clientKey is the first X-Forwarded-For hop, then X-Real-IP, then RemoteAddr.
func clientKey(r *http.Request) string {
	if xff := r.Header.Get("X-Forwarded-For"); xff != "" {
		if first := strings.TrimSpace(strings.Split(xff, ",")[0]); first != "" {
			return first
		}
	}
	if rip := strings.TrimSpace(r.Header.Get("X-Real-IP")); rip != "" {
		return rip
	}
	if host, _, err := net.SplitHostPort(r.RemoteAddr); err == nil {
		return host
	}
	return r.RemoteAddr
}

func (rl *rateLimiter) middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !rl.allow(clientKey(r), time.Now()) {
			w.Header().Set("Retry-After", "1")
			http.Error(w, "rate limited", http.StatusTooManyRequests)
			return
		}
		next.ServeHTTP(w, r)
	})
}
