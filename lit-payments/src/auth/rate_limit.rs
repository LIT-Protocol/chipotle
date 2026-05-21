//! Per-email rate limiter for the magic-link request endpoint.
//!
//! Bounds two attack surfaces flagged by review on this PR:
//! - Spam: an attacker who knows an operator's email could POST `/auth/request`
//!   in a tight loop and flood that inbox / burn Resend quota.
//! - Enumeration: the response shape is the same for operators and non-operators,
//!   but if only operator emails triggered a send we'd still leak membership via
//!   timing. The rate limiter is checked *before* the DB lookup so a rate-limited
//!   request returns at constant time regardless of allowlist status.

use std::time::Duration;

use moka::future::Cache;

/// One-minute cooldown per email. Long enough to defeat naive spam, short enough
/// that a legit operator who fat-fingers their email and retries isn't annoyed.
const COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct RateLimiter {
    seen: Cache<String, ()>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            seen: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(COOLDOWN)
                .build(),
        }
    }

    /// Returns `true` if this email is currently rate-limited (a recent request
    /// is still within the cooldown window).
    ///
    /// On a non-limited call, records the request and the next call within
    /// `COOLDOWN` will return `true`.
    pub async fn check_and_record(&self, email_lowercased: &str) -> bool {
        if self.seen.get(email_lowercased).await.is_some() {
            return true;
        }
        self.seen.insert(email_lowercased.to_string(), ()).await;
        false
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_request_is_allowed() {
        let rl = RateLimiter::new();
        assert!(!rl.check_and_record("a@b.com").await);
    }

    #[tokio::test]
    async fn second_request_within_window_is_limited() {
        let rl = RateLimiter::new();
        assert!(!rl.check_and_record("a@b.com").await);
        assert!(rl.check_and_record("a@b.com").await);
    }

    #[tokio::test]
    async fn different_emails_are_independent() {
        let rl = RateLimiter::new();
        assert!(!rl.check_and_record("a@b.com").await);
        assert!(!rl.check_and_record("c@d.com").await);
    }
}
