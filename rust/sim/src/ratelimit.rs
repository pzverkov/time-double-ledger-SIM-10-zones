//! Per-client token-bucket rate limiter for the public write path.
//!
//! Keyed by client IP. By default the key is the peer address; forwarding
//! headers (X-Forwarded-For / X-Real-IP) are honored only when `trust_proxy` is
//! set, i.e. the operator asserts the service sits behind a trusted proxy that
//! sets them. Trusting those headers unconditionally would let a direct caller
//! spoof a fresh key per request and bypass the limit. `rps` tokens accrue per
//! second up to a `burst` ceiling; a request costs one token. Disabled when
//! `rps == 0` (used by the contract-test stack so fuzzing is not throttled).
//! App-level limiting is a backstop; a real edge deployment should also
//! rate-limit at the gateway.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::{extract::Request, middleware::Next};
use std::sync::Arc;

/// Hard ceiling on tracked clients, bounding memory under a distinct-key flood.
const MAX_BUCKETS: usize = 8192;
const IDLE_TTL: Duration = Duration::from_secs(60);

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct RateLimiter {
    rps: f64,
    burst: f64,
    trust_proxy: bool,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new(rps: u32, burst: u32, trust_proxy: bool) -> Self {
        Self {
            rps: rps as f64,
            burst: burst as f64,
            trust_proxy,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.rps > 0.0
    }

    /// Refill the client's bucket for the elapsed time and consume one token.
    /// Returns true when the request is allowed.
    pub fn check(&self, key: &str, now: Instant) -> bool {
        if !self.enabled() {
            return true;
        }
        let mut buckets = self.buckets.lock().unwrap();
        // Bound the map: only when a new key would push us past the cap, drop
        // idle entries, then evict the oldest if still full.
        if buckets.len() >= MAX_BUCKETS && !buckets.contains_key(key) {
            buckets.retain(|_, b| now.duration_since(b.last) < IDLE_TTL);
            if buckets.len() >= MAX_BUCKETS
                && let Some(oldest) = buckets
                    .iter()
                    .min_by_key(|(_, b)| b.last)
                    .map(|(k, _)| k.clone())
            {
                buckets.remove(&oldest);
            }
        }
        let b = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.burst,
            last: now,
        });
        let elapsed = now.duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * self.rps).min(self.burst);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Client key: the peer address, unless `trust_proxy` is set, in which case
    /// the first X-Forwarded-For hop then X-Real-IP take precedence.
    fn client_key(&self, headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
        if self.trust_proxy {
            if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
                && let Some(first) = xff.split(',').next()
                && !first.trim().is_empty()
            {
                return first.trim().to_string();
            }
            if let Some(rip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok())
                && !rip.trim().is_empty()
            {
                return rip.trim().to_string();
            }
        }
        peer.map(|p| p.ip().to_string())
            .unwrap_or_else(|| "global".to_string())
    }
}

/// axum middleware enforcing the limiter; returns 429 when the bucket is empty.
pub async fn rate_limit(
    State(limiter): State<Arc<RateLimiter>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let key = limiter.client_key(req.headers(), peer);
    if limiter.check(&key, Instant::now()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(rps: u32, trust_proxy: bool) -> RateLimiter {
        RateLimiter::new(rps, rps, trust_proxy)
    }

    #[test]
    fn disabled_limiter_always_allows() {
        let rl = limiter(0, false);
        let now = Instant::now();
        for _ in 0..1000 {
            assert!(rl.check("a", now));
        }
    }

    #[test]
    fn allows_burst_then_denies_until_refill() {
        let rl = RateLimiter::new(10, 5, false);
        let t0 = Instant::now();
        for _ in 0..5 {
            assert!(rl.check("ip1", t0));
        }
        assert!(!rl.check("ip1", t0));
        let t1 = t0 + Duration::from_secs(1);
        for _ in 0..5 {
            assert!(rl.check("ip1", t1));
        }
        assert!(!rl.check("ip1", t1));
    }

    #[test]
    fn buckets_are_per_key() {
        let rl = limiter(1, false);
        let t0 = Instant::now();
        assert!(rl.check("ip1", t0));
        assert!(!rl.check("ip1", t0));
        assert!(rl.check("ip2", t0));
    }

    #[test]
    fn ignores_forwarding_headers_unless_trusted() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7".parse().unwrap());
        let peer = Some("198.51.100.9:5000".parse::<SocketAddr>().unwrap());

        let untrusted = limiter(1, false);
        assert_eq!(untrusted.client_key(&h, peer), "198.51.100.9");

        let trusted = limiter(1, true);
        assert_eq!(trusted.client_key(&h, peer), "203.0.113.7");
    }

    #[test]
    fn trusted_proxy_uses_leftmost_xff_hop() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        let trusted = limiter(1, true);
        assert_eq!(trusted.client_key(&h, None), "203.0.113.7");
    }
}
