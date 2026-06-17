package web

import (
	"math"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

// Per-client token-bucket rate limiter for the public write path. By default
// the key is the peer address; forwarding headers (X-Forwarded-For / X-Real-IP)
// are honored only when trustProxy is set, i.e. the service sits behind a
// trusted proxy that sets them. Trusting those headers unconditionally would
// let a direct caller spoof a fresh key per request and bypass the limit. rps
// tokens accrue per second up to a burst ceiling; a request costs one token.
// Disabled when rps == 0 (the contract-test stack sets 0 so fuzzing is not
// throttled). App-level limiting is a backstop; a real edge deployment should
// also rate-limit at the gateway.

const (
	rlMaxBuckets = 8192
	rlIdleTTL    = 60 * time.Second
)

type rlBucket struct {
	tokens float64
	last   time.Time
}

type rateLimiter struct {
	rps        float64
	burst      float64
	trustProxy bool
	mu         sync.Mutex
	buckets    map[string]*rlBucket
}

func newRateLimiter(rps int, trustProxy bool) *rateLimiter {
	return &rateLimiter{
		rps:        float64(rps),
		burst:      float64(rps),
		trustProxy: trustProxy,
		buckets:    map[string]*rlBucket{},
	}
}

func (rl *rateLimiter) enabled() bool { return rl.rps > 0 }

// allow refills the client's bucket for the elapsed time and consumes one token.
func (rl *rateLimiter) allow(key string, now time.Time) bool {
	if !rl.enabled() {
		return true
	}
	rl.mu.Lock()
	defer rl.mu.Unlock()
	// Bound the map: only when a new key would push us past the cap, drop idle
	// entries, then evict the oldest if still full.
	if _, ok := rl.buckets[key]; !ok && len(rl.buckets) >= rlMaxBuckets {
		rl.evictLocked(now)
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

func (rl *rateLimiter) evictLocked(now time.Time) {
	for k, b := range rl.buckets {
		if now.Sub(b.last) > rlIdleTTL {
			delete(rl.buckets, k)
		}
	}
	if len(rl.buckets) < rlMaxBuckets {
		return
	}
	var oldestKey string
	var oldest time.Time
	for k, b := range rl.buckets {
		if oldestKey == "" || b.last.Before(oldest) {
			oldestKey, oldest = k, b.last
		}
	}
	if oldestKey != "" {
		delete(rl.buckets, oldestKey)
	}
}

// clientKey is the peer address, unless trustProxy is set, in which case the
// first X-Forwarded-For hop then X-Real-IP take precedence.
func (rl *rateLimiter) clientKey(r *http.Request) string {
	if rl.trustProxy {
		if xff := r.Header.Get("X-Forwarded-For"); xff != "" {
			if first := strings.TrimSpace(strings.Split(xff, ",")[0]); first != "" {
				return first
			}
		}
		if rip := strings.TrimSpace(r.Header.Get("X-Real-IP")); rip != "" {
			return rip
		}
	}
	if host, _, err := net.SplitHostPort(r.RemoteAddr); err == nil {
		return host
	}
	return r.RemoteAddr
}

func (rl *rateLimiter) middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !rl.allow(rl.clientKey(r), time.Now()) {
			w.Header().Set("Retry-After", "1")
			http.Error(w, "rate limited", http.StatusTooManyRequests)
			return
		}
		next.ServeHTTP(w, r)
	})
}
