//! In-memory webhook admission controls for the single-node v1 deployment.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allowed,
    Limited,
}

#[derive(Debug, Clone)]
pub struct WebhookRateLimiter {
    window: Duration,
    buckets: std::sync::Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl WebhookRateLimiter {
    pub fn new() -> Self {
        Self::with_window(Duration::from_secs(60))
    }

    pub fn with_window(window: Duration) -> Self {
        Self {
            window,
            buckets: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn check_and_record(&self, key: impl Into<String>, limit: u32) -> RateLimitDecision {
        if limit == 0 {
            return RateLimitDecision::Limited;
        }

        let now = Instant::now();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets.entry(key.into()).or_default();
        bucket.retain(|seen| *seen >= cutoff);

        if bucket.len() >= limit as usize {
            return RateLimitDecision::Limited;
        }

        bucket.push(now);
        RateLimitDecision::Allowed
    }
}

impl Default for WebhookRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_until_limit_then_rejects() {
        let limiter = WebhookRateLimiter::new();
        assert_eq!(
            limiter.check_and_record("ip:1.2.3.4", 2).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            limiter.check_and_record("ip:1.2.3.4", 2).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            limiter.check_and_record("ip:1.2.3.4", 2).await,
            RateLimitDecision::Limited
        );
    }

    #[tokio::test]
    async fn independent_keys_have_independent_buckets() {
        let limiter = WebhookRateLimiter::new();
        assert_eq!(
            limiter.check_and_record("a", 1).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            limiter.check_and_record("b", 1).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            limiter.check_and_record("a", 1).await,
            RateLimitDecision::Limited
        );
    }

    #[tokio::test]
    async fn zero_limit_rejects() {
        let limiter = WebhookRateLimiter::new();
        assert_eq!(
            limiter.check_and_record("a", 0).await,
            RateLimitDecision::Limited
        );
    }
}
