//! Gateway-side enforcement of per-key spending rules (Lambda parity).
//!
//! Only reached when a key's on-chain `hasSpendingRules` flag is set (see
//! `accounts::can_execute_action_with_spending_rules`), so keys without rules
//! pay zero added latency. For a flagged key, the rules + current rolling spend
//! are fetched (cached) from lit-payments' `/internal` endpoints and enforced
//! before execution:
//!
//! - **rolling spend cap** (402 when reached),
//! - **rate limit** — per-node token bucket (429),
//! - **concurrency cap** — per-node in-flight counter (429).
//!
//! Spend is recorded back to lit-payments off the response path. Counters are
//! in-process and per-node — acceptable because the durable spend cap is the
//! real backstop. See `plans/chipotle-lambda-parity.md`.
//!
//! Inert until configured: if `LIT_PAYMENTS_INTERNAL_URL` /
//! `INTERNAL_SERVICE_TOKEN` are unset, `admit` always allows and records nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use moka::future::Cache;
use serde::Deserialize;

use crate::core::v1::helpers::api_status::ApiStatus;

/// SWR-ish freshness for the rules cache (kept short; only flagged keys pay it).
const RULES_CACHE_TTL: Duration = Duration::from_secs(30);
const RULES_CACHE_CAPACITY: u64 = 100_000;
/// Bound the hot-path cold-miss fetch so a slow lit-payments can't stall a call.
const FETCH_TIMEOUT: Duration = Duration::from_secs(2);
/// Per-second execution cost, mirrored from the Stripe charge rate so the
/// per-key counter tracks roughly what the account is billed.
const COST_PER_SECOND_CENTS: i64 = crate::stripe::COST_LIT_ACTION_PER_SECOND_CENTS;

/// The rules the gateway enforces for one key (subset of the lit-payments row).
#[derive(Debug, Clone, Deserialize)]
pub struct RuleSet {
    pub spend_cap_cents: Option<i64>,
    pub spend_window_seconds: Option<i64>,
    pub rate_limit_rps: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub max_concurrency: Option<i32>,
    pub enabled: bool,
}

/// Shape of `GET /internal/spending-rules/<hash>` from lit-payments.
#[derive(Debug, Deserialize)]
struct RulesWithUsage {
    rules: RuleSet,
    usage: Option<UsageRow>,
}

#[derive(Debug, Deserialize)]
struct UsageRow {
    spent_cents: i64,
}

/// Local rolling-spend counter, seeded from lit-payments and incremented on each
/// charge. Window resets locally when it elapses (fixed-window).
#[derive(Debug)]
struct Usage {
    window_start: Instant,
    spent_cents: i64,
}

impl Usage {
    /// Reset the window if `window` has elapsed since it started.
    fn roll(&mut self, now: Instant, window: Duration) {
        if now.duration_since(self.window_start) >= window {
            self.window_start = now;
            self.spent_cents = 0;
        }
    }
}

/// Per-key token bucket for rate limiting.
#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl Bucket {
    /// Refill by elapsed time and try to consume one token. Returns true if allowed.
    fn try_take(&mut self, now: Instant, rps: f64, burst: f64) -> bool {
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rps).min(burst);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

struct Inner {
    enabled: bool,
    base_url: String,
    token: String,
    http: reqwest::Client,
    rules_cache: Cache<String, Option<Arc<RuleSet>>>,
    usage: Mutex<HashMap<String, Usage>>,
    buckets: Mutex<HashMap<String, Bucket>>,
    concurrency: Mutex<HashMap<String, u32>>,
}

/// Shared, cheaply-clonable spending-rules enforcer. Managed in Rocket state.
#[derive(Clone)]
pub struct SpendingRulesState {
    inner: Arc<Inner>,
}

impl SpendingRulesState {
    /// Build from env. Enforcement is enabled only when both
    /// `LIT_PAYMENTS_INTERNAL_URL` and `INTERNAL_SERVICE_TOKEN` are set;
    /// otherwise this is fully inert.
    pub fn from_env() -> Self {
        let base_url = std::env::var("LIT_PAYMENTS_INTERNAL_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        let token = std::env::var("INTERNAL_SERVICE_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let enabled = base_url.is_some() && token.is_some();
        if enabled {
            tracing::info!("spending_rules: enforcement enabled");
        } else {
            tracing::info!(
                "spending_rules: disabled (set LIT_PAYMENTS_INTERNAL_URL + INTERNAL_SERVICE_TOKEN to enable)"
            );
        }

        let http = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .build()
            .unwrap_or_default();

        Self {
            inner: Arc::new(Inner {
                enabled,
                base_url: base_url.unwrap_or_default(),
                token: token.unwrap_or_default(),
                http,
                rules_cache: Cache::builder()
                    .max_capacity(RULES_CACHE_CAPACITY)
                    .time_to_live(RULES_CACHE_TTL)
                    .build(),
                usage: Mutex::new(HashMap::new()),
                buckets: Mutex::new(HashMap::new()),
                concurrency: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Enforce a flagged key's rules before execution. Returns an [`Admission`]
    /// the caller holds across execution (releasing any concurrency permit on
    /// drop) and calls [`Admission::record_spend`] on afterwards.
    ///
    /// `has_spending_rules` is the on-chain gate; pass it so we skip all work
    /// (and the lit-payments round trip) for keys without rules.
    pub async fn admit(&self, api_key: &str, has_spending_rules: bool) -> Result<Admission, ApiStatus> {
        if !self.inner.enabled || !has_spending_rules {
            return Ok(Admission(AdmissionInner::Noop));
        }
        let hash = key_hash(api_key);
        let rules = match self.fetch_rules(&hash).await {
            Some(r) if r.enabled => r,
            // no row, disabled, or fetch failed → don't block
            _ => return Ok(Admission(AdmissionInner::Noop)),
        };

        let now = Instant::now();

        // Rate limit.
        if let (Some(rps), Some(burst)) = (rules.rate_limit_rps, rules.rate_limit_burst) {
            let mut buckets = self.inner.buckets.lock().unwrap();
            let bucket = buckets.entry(hash.clone()).or_insert(Bucket {
                tokens: burst as f64,
                last_refill: now,
            });
            if !bucket.try_take(now, rps as f64, burst as f64) {
                return Err(ApiStatus::too_many_requests(format!(
                    "rate limit exceeded for this API key ({rps} rps)"
                )));
            }
        }

        // Rolling spend cap.
        if let (Some(cap), Some(window)) = (rules.spend_cap_cents, rules.spend_window_seconds) {
            let mut usage = self.inner.usage.lock().unwrap();
            let u = usage.entry(hash.clone()).or_insert(Usage {
                window_start: now,
                spent_cents: 0,
            });
            u.roll(now, Duration::from_secs(window.max(0) as u64));
            if u.spent_cents >= cap {
                return Err(ApiStatus::payment_required(format!(
                    "spending cap reached for this API key ({cap} cents / {window}s window)"
                )));
            }
        }

        // Concurrency (acquire last, so a rejection above never leaks a permit).
        let concurrency_guard = if let Some(max) = rules.max_concurrency {
            let mut counts = self.inner.concurrency.lock().unwrap();
            let count = counts.entry(hash.clone()).or_insert(0);
            if *count >= max as u32 {
                return Err(ApiStatus::too_many_requests(format!(
                    "max concurrent executions reached for this API key ({max})"
                )));
            }
            *count += 1;
            Some(ConcurrencyGuard {
                inner: self.inner.clone(),
                key_hash: hash.clone(),
            })
        } else {
            None
        };

        Ok(Admission(AdmissionInner::Active {
            inner: self.inner.clone(),
            key_hash: hash,
            window_secs: rules.spend_window_seconds,
            _concurrency: concurrency_guard,
        }))
    }

    /// Fetch a key's rules (cached, TTL). `Ok(None)` (no row / disabled) is
    /// cached so we don't refetch every request; network errors are not.
    ///
    /// TODO: upgrade to serve-stale-while-revalidate (background refresh), like
    /// `stripe::get_credit_balance`, so the cold tick never blocks the hot path.
    async fn fetch_rules(&self, hash: &str) -> Option<Arc<RuleSet>> {
        let cache = self.inner.rules_cache.clone();
        let inner = self.inner.clone();
        let hash_owned = hash.to_string();
        cache
            .try_get_with(hash.to_string(), async move {
                let url = format!("{}/internal/spending-rules/{}", inner.base_url, hash_owned);
                let resp = inner
                    .http
                    .get(&url)
                    .bearer_auth(&inner.token)
                    .send()
                    .await
                    .map_err(|e| format!("spending_rules fetch failed: {e}"))?;
                if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok::<_, String>(None);
                }
                if !resp.status().is_success() {
                    return Err(format!("spending_rules fetch status {}", resp.status()));
                }
                let body: RulesWithUsage = resp
                    .json()
                    .await
                    .map_err(|e| format!("spending_rules decode failed: {e}"))?;
                // Seed the local rolling counter from the server's value.
                if let Some(usage) = &body.usage {
                    seed_usage(&inner, &hash_owned, usage.spent_cents);
                }
                Ok(Some(Arc::new(body.rules)))
            })
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("spending_rules: {e}");
                None // fail open on transient error — never block a real call
            })
    }

    fn add_local_spend(&self, hash: &str, cents: i64, window: i64) {
        let now = Instant::now();
        let mut usage = self.inner.usage.lock().unwrap();
        let u = usage.entry(hash.to_string()).or_insert(Usage {
            window_start: now,
            spent_cents: 0,
        });
        u.roll(now, Duration::from_secs(window.max(0) as u64));
        u.spent_cents = u.spent_cents.saturating_add(cents);
    }

    /// Fire-and-forget POST of `cents` to lit-payments' rolling counter.
    fn spawn_record(&self, hash: String, cents: i64, window: i64) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let url = format!("{}/internal/spending-usage/{}/charge", inner.base_url, hash);
            let res = inner
                .http
                .post(&url)
                .bearer_auth(&inner.token)
                .json(&serde_json::json!({ "cents": cents, "window_seconds": window }))
                .send()
                .await;
            if let Err(e) = res {
                tracing::warn!("spending_rules: record_spend POST failed: {e}");
            }
        });
    }
}

/// Seed/refresh the local counter from the server's value, taking the max so a
/// background refresh never undoes a local optimistic increment (cf.
/// `stripe::should_update_balance_cache`).
fn seed_usage(inner: &Inner, hash: &str, server_spent: i64) {
    let now = Instant::now();
    let mut usage = inner.usage.lock().unwrap();
    let entry = usage.entry(hash.to_string()).or_insert(Usage {
        window_start: now,
        spent_cents: 0,
    });
    entry.spent_cents = entry.spent_cents.max(server_spent);
}

/// Held across execution. On drop, releases any concurrency permit. Call
/// [`Admission::record_seconds`] after execution to bill the rolling counter.
pub struct Admission(AdmissionInner);

enum AdmissionInner {
    Noop,
    Active {
        inner: Arc<Inner>,
        key_hash: String,
        window_secs: Option<i64>,
        _concurrency: Option<ConcurrencyGuard>,
    },
}

impl Admission {
    /// Record `seconds` of execution against the key's rolling spend (local +
    /// async POST to lit-payments). No-op when there is no spend cap to enforce.
    pub fn record_seconds(&self, seconds: u64) {
        if let AdmissionInner::Active {
            inner,
            key_hash,
            window_secs: Some(window),
            ..
        } = &self.0
        {
            let cents = (seconds.max(1) as i64).saturating_mul(COST_PER_SECOND_CENTS);
            let state = SpendingRulesState {
                inner: inner.clone(),
            };
            state.add_local_spend(key_hash, cents, *window);
            state.spawn_record(key_hash.clone(), cents, *window);
        }
    }
}

/// RAII concurrency permit: decrements the in-flight count on drop.
pub struct ConcurrencyGuard {
    inner: Arc<Inner>,
    key_hash: String,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        let mut counts = self.inner.concurrency.lock().unwrap();
        if let Some(c) = counts.get_mut(&self.key_hash) {
            *c = c.saturating_sub(1);
        }
    }
}

/// The key's on-chain identity hash as 0x-prefixed 32-byte lowercase hex —
/// matching lit-payments' `canonical_key_hash`.
fn key_hash(api_key: &str) -> String {
    let h = crate::utils::parse_with_hash::api_key_hash(api_key);
    format!("0x{:0>64}", format!("{h:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_burst_then_throttles() {
        let now = Instant::now();
        let mut b = Bucket {
            tokens: 2.0,
            last_refill: now,
        };
        // Two tokens available, no time passing → two allowed, third denied.
        assert!(b.try_take(now, 1.0, 2.0));
        assert!(b.try_take(now, 1.0, 2.0));
        assert!(!b.try_take(now, 1.0, 2.0));
    }

    #[test]
    fn bucket_refills_over_time() {
        let now = Instant::now();
        let mut b = Bucket {
            tokens: 0.0,
            last_refill: now,
        };
        assert!(!b.try_take(now, 10.0, 10.0));
        // 0.5s at 10rps → ~5 tokens.
        let later = now + Duration::from_millis(500);
        assert!(b.try_take(later, 10.0, 10.0));
    }

    #[test]
    fn usage_window_resets_after_elapse() {
        let now = Instant::now();
        let mut u = Usage {
            window_start: now,
            spent_cents: 500,
        };
        u.roll(now + Duration::from_secs(5), Duration::from_secs(10));
        assert_eq!(u.spent_cents, 500); // within window
        u.roll(now + Duration::from_secs(11), Duration::from_secs(10));
        assert_eq!(u.spent_cents, 0); // window elapsed → reset
    }

    #[test]
    fn key_hash_is_0x_64_lowercase_hex() {
        let h = key_hash("some-api-key");
        assert!(h.starts_with("0x"));
        assert_eq!(h.len(), 66);
        assert!(h[2..].bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(h, h.to_lowercase());
    }
}
