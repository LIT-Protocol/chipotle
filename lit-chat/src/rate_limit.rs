//! Per-client-IP token-bucket rate limiting for the unauthenticated /
//! cheaply-authenticated chat surface. Port of the CPL-367 limiter
//! (lit-api-server/src/core/v1/guards/rate_limit.rs) with lit-chat env names.
//!
//! Trust model: behind dstack-ingress the socket peer is always the proxy;
//! the per-IP key is only meaningful if the ingress sets a trustworthy
//! real-IP header (Rocket `ip_header`, default X-Real-IP). Same caveat as
//! CPL-367, disclosed rather than solved.

use moka::future::Cache;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_BURST: f64 = 20.0;
const DEFAULT_PER_MIN: f64 = 20.0;
const DEFAULT_IDLE_SECS: u64 = 3600;
/// Bound memory under an IP-rotating flood; LRU-evicted beyond this.
const MAX_TRACKED_IPS: u64 = 100_000;

#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn full(capacity: f64, now: Instant) -> Self {
        Self {
            tokens: capacity,
            last_refill: now,
        }
    }

    fn try_consume(&mut self, capacity: f64, refill_per_sec: f64, now: Instant) -> bool {
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        self.tokens = (self.tokens + elapsed * refill_per_sec).min(capacity);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    enabled: bool,
    capacity: f64,
    refill_per_sec: f64,
    buckets: Cache<IpAddr, Arc<Mutex<TokenBucket>>>,
    /// Shared bucket for requests with no resolvable client IP, so a missing
    /// IP can't sidestep the limit.
    no_ip_bucket: Arc<Mutex<TokenBucket>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        let enabled = std::env::var("LIT_CHAT_RATE_LIMIT_ENABLED")
            .ok()
            .map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no"))
            .unwrap_or(true);
        let capacity = env_f64("LIT_CHAT_RATE_LIMIT_BURST", DEFAULT_BURST).max(1.0);
        let per_min = env_f64("LIT_CHAT_RATE_LIMIT_PER_MIN", DEFAULT_PER_MIN).max(0.0);
        let idle_secs = std::env::var("LIT_CHAT_RATE_LIMIT_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_IDLE_SECS);
        tracing::info!(
            enabled,
            capacity,
            per_min,
            idle_secs,
            "chat per-IP rate limiter"
        );
        Self::build(enabled, capacity, per_min / 60.0, idle_secs)
    }

    fn build(enabled: bool, capacity: f64, refill_per_sec: f64, idle_secs: u64) -> Self {
        // Clamp >= 1s: a 0s idle TTL would hand every request a fresh full
        // bucket and silently disable the limit.
        let idle_secs = idle_secs.max(1);
        let buckets = Cache::builder()
            .max_capacity(MAX_TRACKED_IPS)
            .time_to_idle(Duration::from_secs(idle_secs))
            .build();
        Self {
            enabled,
            capacity,
            refill_per_sec,
            buckets,
            no_ip_bucket: Arc::new(Mutex::new(TokenBucket::full(capacity, Instant::now()))),
        }
    }

    pub async fn check(&self, ip: Option<IpAddr>) -> bool {
        if !self.enabled {
            return true;
        }
        let now = Instant::now();
        let bucket = match ip {
            Some(ip) => {
                let capacity = self.capacity;
                self.buckets
                    .get_with(ip, async move {
                        Arc::new(Mutex::new(TokenBucket::full(capacity, now)))
                    })
                    .await
            }
            None => self.no_ip_bucket.clone(),
        };
        let mut b = bucket.lock().await;
        b.try_consume(self.capacity, self.refill_per_sec, now)
    }
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Request guard for stream-start and other abuse-magnet routes. Fails open
/// when no limiter is managed (tests), 429s when the bucket is dry.
pub struct StreamRateLimit;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for StreamRateLimit {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(limiter) = req.rocket().state::<RateLimiter>() else {
            return Outcome::Success(StreamRateLimit);
        };
        if limiter.check(req.client_ip()).await {
            Outcome::Success(StreamRateLimit)
        } else {
            tracing::warn!(client_ip = ?req.client_ip(), "chat request throttled: per-IP limit");
            Outcome::Error((Status::TooManyRequests, ()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(last: u8) -> Option<IpAddr> {
        Some(IpAddr::from([10, 0, 0, last]))
    }

    #[tokio::test]
    async fn burst_then_throttle() {
        let l = RateLimiter::build(true, 3.0, 0.0, 60);
        assert!(l.check(ip(1)).await);
        assert!(l.check(ip(1)).await);
        assert!(l.check(ip(1)).await);
        assert!(!l.check(ip(1)).await);
        // Other IPs unaffected.
        assert!(l.check(ip(2)).await);
    }

    #[tokio::test]
    async fn disabled_always_admits() {
        let l = RateLimiter::build(false, 1.0, 0.0, 60);
        for _ in 0..10 {
            assert!(l.check(ip(1)).await);
        }
    }

    #[tokio::test]
    async fn missing_ip_shares_a_bucket() {
        let l = RateLimiter::build(true, 2.0, 0.0, 60);
        assert!(l.check(None).await);
        assert!(l.check(None).await);
        assert!(!l.check(None).await);
    }
}
