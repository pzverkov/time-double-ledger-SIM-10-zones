//! Per-client token-bucket rate limiter for the public write path.
//!
//! Keyed by client IP (X-Forwarded-For / X-Real-IP, falling back to the peer
//! address). `rps` tokens accrue per second up to a `burst` ceiling; a request
//! costs one token. Disabled when `rps == 0` (used by the contract-test stack so
//! fuzzing is not throttled). App-level limiting is a backstop; a real edge
//! deployment should also rate-limit at the gateway.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::{extract::Request, middleware::Next};
use std::sync::Arc;

/// Drop idle buckets once the map grows past this, bounding memory under churn.
const GC_THRESHOLD: usize = 8192;
const IDLE_TTL: Duration = Duration::from_secs(60);

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct RateLimiter {
    rps: f64,
    burst: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new(rps: u32, burst: u32) -> Self {
        Self {
            rps: rps as f64,
            burst: burst as f64,
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
        if buckets.len() > GC_THRESHOLD {
            buckets.retain(|_, b| now.duration_since(b.last) < IDLE_TTL);
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
}

/// Client key: first X-Forwarded-For hop, then X-Real-IP, then the peer address.
fn client_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
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
    peer.map(|p| p.ip().to_string())
        .unwrap_or_else(|| "global".to_string())
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
    let key = client_key(req.headers(), peer);
    if limiter.check(&key, Instant::now()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_limiter_always_allows() {
        let rl = RateLimiter::new(0, 0);
        let now = Instant::now();
        for _ in 0..1000 {
            assert!(rl.check("a", now));
        }
    }

    #[test]
    fn allows_burst_then_denies_until_refill() {
        let rl = RateLimiter::new(10, 5);
        let t0 = Instant::now();
        // burst of 5 succeeds, 6th is denied at the same instant
        for _ in 0..5 {
            assert!(rl.check("ip1", t0));
        }
        assert!(!rl.check("ip1", t0));
        // after 1s, 10 tokens accrue but cap at burst (5), so 5 more allowed
        let t1 = t0 + Duration::from_secs(1);
        for _ in 0..5 {
            assert!(rl.check("ip1", t1));
        }
        assert!(!rl.check("ip1", t1));
    }

    #[test]
    fn buckets_are_per_key() {
        let rl = RateLimiter::new(1, 1);
        let t0 = Instant::now();
        assert!(rl.check("ip1", t0));
        assert!(!rl.check("ip1", t0));
        // a different client has its own bucket
        assert!(rl.check("ip2", t0));
    }

    #[test]
    fn xff_takes_precedence_for_key() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(client_key(&h, None), "203.0.113.7");
    }
}
