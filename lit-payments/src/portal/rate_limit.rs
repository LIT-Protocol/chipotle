//! Per-client-IP rate limiting for the public customer-preview endpoint.
//!
//! `GET /api/customer/preview?wallet=…` (CPL-376) is unauthenticated by design —
//! it backs the public pay-with-LITKEY page so a payer can confirm which account
//! they're about to credit. But that also lets anyone enumerate public on-chain
//! wallets and harvest the wallet↔email linkage as a de-anonymization dataset.
//! The email is already masked before it leaves the server (see
//! [`super::routes::mask_email`]); this limiter is the second half of the fix:
//! bound how fast a single source can drive the endpoint.
//!
//! It's a **token-bucket per client IP** — a burst of up to `capacity` requests
//! is allowed, after which the bucket refills at `refill_per_sec`. Mirrors the
//! `new_account` limiter in `lit-api-server` (CPL-367); see that module for the
//! full rationale.
//!
//! ## Trust model — read before relying on this
//!
//! The limiter keys on [`rocket::Request::client_ip`], which reads Rocket's
//! configured `ip_header` (default `X-Real-IP`) and falls back to the socket
//! peer address. lit-payments runs behind an edge proxy (Railway), so the socket
//! peer is the proxy — the per-IP key is only meaningful if the proxy sets a
//! **trustworthy** real-IP header:
//!
//! - If the proxy overwrites `X-Real-IP` with the true client address, the
//!   per-IP bucket works as intended.
//! - If it passes a client-supplied header through untouched, an attacker
//!   rotates the header value and bypasses the per-IP key.
//! - If it sets nothing, `client_ip()` returns the proxy address and every
//!   caller shares one bucket — the limit degrades to a global cap (still a
//!   meaningful backstop against enumeration, but it can throttle honest payers
//!   during an attack).
//!
//! Every failure mode is safe-by-construction (never *more* permissive than a
//! global cap). Because the email is masked regardless, the harvesting value of
//! the endpoint is low even without a per-IP key, so the defaults are generous.
//!
//! ## Configuration (env vars, all optional)
//!
//! - `PREVIEW_RATE_LIMIT_ENABLED`   — `false` disables the guard (default on)
//! - `PREVIEW_RATE_LIMIT_BURST`     — bucket capacity / max burst (default 60)
//! - `PREVIEW_RATE_LIMIT_PER_MIN`   — sustained refill, requests/minute (default 60)
//! - `PREVIEW_RATE_LIMIT_IDLE_SECS` — evict an IP's bucket after this idle time (default 3600)

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use moka::future::Cache;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use tokio::sync::Mutex;

/// Default burst capacity (requests allowed back-to-back before throttling).
const DEFAULT_BURST: f64 = 60.0;
/// Default sustained rate, in requests per minute, at which the bucket refills.
const DEFAULT_PER_MIN: f64 = 60.0;
/// Default idle eviction for a per-IP bucket.
const DEFAULT_IDLE_SECS: u64 = 3600;
/// Hard ceiling on distinct IPs tracked at once, to bound memory under an
/// IP-rotating flood. LRU-evicted by moka beyond this.
const MAX_TRACKED_IPS: u64 = 100_000;

/// A classic token bucket. `tokens` accrue at `refill_per_sec` up to `capacity`;
/// each admitted request consumes one.
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// A fresh bucket starts full so a first-time caller is never throttled.
    fn full(capacity: f64, now: Instant) -> Self {
        Self {
            tokens: capacity,
            last_refill: now,
        }
    }

    /// Refill for elapsed time (capped at `capacity`), then try to spend one
    /// token. Returns `true` if the request is admitted.
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

/// Per-IP token-bucket rate limiter, registered as Rocket managed state.
///
/// Cheap to clone — the clone shares the same underlying bucket cache (moka
/// `Cache` and the no-IP `Arc<Mutex<..>>` are reference-counted).
#[derive(Clone)]
pub struct PreviewRateLimiter {
    enabled: bool,
    capacity: f64,
    refill_per_sec: f64,
    /// One bucket per client IP; idle IPs are evicted after `idle_secs`.
    buckets: Cache<IpAddr, Arc<Mutex<TokenBucket>>>,
    /// Shared bucket for requests whose client IP can't be resolved, so a
    /// missing IP can't sidestep the limit entirely.
    no_ip_bucket: Arc<Mutex<TokenBucket>>,
}

impl PreviewRateLimiter {
    /// Build the limiter, reading thresholds from the environment.
    pub fn new() -> Self {
        let enabled = std::env::var("PREVIEW_RATE_LIMIT_ENABLED")
            .ok()
            .map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no"))
            .unwrap_or(true);

        let capacity = env_f64("PREVIEW_RATE_LIMIT_BURST", DEFAULT_BURST).max(1.0);
        let per_min = env_f64("PREVIEW_RATE_LIMIT_PER_MIN", DEFAULT_PER_MIN).max(0.0);
        let refill_per_sec = per_min / 60.0;

        let idle_secs = std::env::var("PREVIEW_RATE_LIMIT_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_IDLE_SECS);

        tracing::info!(
            enabled,
            capacity,
            per_min,
            idle_secs,
            "customer-preview per-IP rate limiter initialized"
        );

        Self::build(enabled, capacity, refill_per_sec, idle_secs)
    }

    fn build(enabled: bool, capacity: f64, refill_per_sec: f64, idle_secs: u64) -> Self {
        // Clamp to >= 1s: a 0s idle TTL would evict every bucket before the
        // next request, so each request would get a fresh full bucket and the
        // guard would silently stop throttling. A misconfigured env var must
        // never disable the limit outright — use PREVIEW_RATE_LIMIT_ENABLED for
        // that.
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

    /// Returns `true` if a request from `ip` is admitted, `false` if it should
    /// be throttled (`429`). A disabled limiter always admits.
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

impl Default for PreviewRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Request guard that throttles the customer-preview endpoint per client IP.
///
/// Runs before the handler body, so a throttled request never reaches the
/// Stripe lookup. Fails **open** when no [`PreviewRateLimiter`] is in managed
/// state (e.g. unit tests that build a bare Rocket) so the guard never blocks a
/// misconfigured server outright.
pub struct PreviewRateLimit;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PreviewRateLimit {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(limiter) = req.rocket().state::<PreviewRateLimiter>() else {
            return Outcome::Success(PreviewRateLimit);
        };

        if limiter.check(req.client_ip()).await {
            Outcome::Success(PreviewRateLimit)
        } else {
            tracing::warn!(
                client_ip = ?req.client_ip(),
                "customer-preview throttled: per-IP rate limit exceeded"
            );
            Outcome::Error((Status::TooManyRequests, ()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_allows_burst_then_throttles() {
        let now = Instant::now();
        let mut b = TokenBucket::full(3.0, now);
        assert!(b.try_consume(3.0, 1.0, now));
        assert!(b.try_consume(3.0, 1.0, now));
        assert!(b.try_consume(3.0, 1.0, now));
        assert!(!b.try_consume(3.0, 1.0, now));
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let start = Instant::now();
        let mut b = TokenBucket::full(2.0, start);
        assert!(b.try_consume(2.0, 1.0, start));
        assert!(b.try_consume(2.0, 1.0, start));
        assert!(!b.try_consume(2.0, 1.0, start));
        let later = start + Duration::from_secs(1);
        assert!(b.try_consume(2.0, 1.0, later));
        assert!(!b.try_consume(2.0, 1.0, later));
    }

    #[tokio::test]
    async fn disabled_limiter_always_admits() {
        let limiter = PreviewRateLimiter::build(false, 1.0, 0.0, 3600);
        let ip = Some("1.2.3.4".parse().unwrap());
        for _ in 0..100 {
            assert!(limiter.check(ip).await);
        }
    }

    #[tokio::test]
    async fn per_ip_buckets_are_independent() {
        let limiter = PreviewRateLimiter::build(true, 2.0, 0.0, 3600);
        let a = Some("10.0.0.1".parse().unwrap());
        let b = Some("10.0.0.2".parse().unwrap());

        assert!(limiter.check(a).await);
        assert!(limiter.check(a).await);
        assert!(!limiter.check(a).await, "IP a exhausted its burst");

        assert!(limiter.check(b).await);
        assert!(limiter.check(b).await);
        assert!(!limiter.check(b).await, "IP b exhausted its own burst");
    }

    #[tokio::test]
    async fn missing_ip_shares_one_bucket() {
        let limiter = PreviewRateLimiter::build(true, 2.0, 0.0, 3600);
        assert!(limiter.check(None).await);
        assert!(limiter.check(None).await);
        assert!(
            !limiter.check(None).await,
            "requests with no resolvable IP share a single bucket"
        );
    }

    /// A `0` idle TTL must not silently disable throttling.
    #[tokio::test]
    async fn zero_idle_ttl_still_throttles() {
        let limiter = PreviewRateLimiter::build(true, 2.0, 0.0, 0);
        let ip = Some("172.16.0.1".parse().unwrap());
        assert!(limiter.check(ip).await);
        assert!(limiter.check(ip).await);
        assert!(
            !limiter.check(ip).await,
            "burst must still be enforced with idle_secs = 0"
        );
    }

    // --- Route-level test: exercise the guard through a real Rocket request. ---

    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};

    #[get("/probe")]
    fn guarded_route(_rl: PreviewRateLimit) -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn guard_returns_429_after_burst_exhausted() {
        let limiter = PreviewRateLimiter::build(true, 2.0, 0.0, 3600);
        let rocket = rocket::build()
            .manage(limiter)
            .mount("/", routes![guarded_route]);
        let client = Client::tracked(rocket).await.expect("valid rocket");

        assert_eq!(client.get("/probe").dispatch().await.status(), Status::Ok);
        assert_eq!(client.get("/probe").dispatch().await.status(), Status::Ok);
        assert_eq!(
            client.get("/probe").dispatch().await.status(),
            Status::TooManyRequests,
            "third request should be throttled"
        );
    }

    #[tokio::test]
    async fn guard_passes_when_limiter_absent() {
        let rocket = rocket::build().mount("/", routes![guarded_route]);
        let client = Client::tracked(rocket).await.expect("valid rocket");
        for _ in 0..10 {
            assert_eq!(client.get("/probe").dispatch().await.status(), Status::Ok);
        }
    }
}
