//! Gateway-side enforcement of per-key spending rules (Lambda parity).
//!
//! Only reached when a key's on-chain `hasSpendingRules` flag is set (see
//! `accounts::can_execute_action_with_spending_rules`), so keys without rules
//! pay zero added latency. For a flagged key, the rules + current rolling spend
//! are fetched (cached) from lit-payments' `/internal` endpoints and enforced
//! before execution:
//!
//! - **origin allowlist** (403 when the browser `Origin` is missing or not
//!   listed — P2.1; trivially spoofed by non-browser clients, so it is
//!   defense-in-depth on top of the limits below),
//! - **rate limit** — per-node token bucket per key (429),
//! - **per-IP rate limit** — per-node token bucket per (key, client IP) (429),
//! - **rolling spend cap** (402 when reached),
//! - **concurrency cap** — per-node in-flight counter (429).
//!
//! Spend is recorded back to lit-payments off the response path. Counters are
//! in-process and per-node — acceptable because the durable spend cap is the
//! real backstop. See `plans/chipotle-lambda-parity.md`.
//!
//! This module also owns the gateway → lit-payments *write* path used by the
//! account-management endpoints (`set_rules` / `delete_rules`), so the
//! lit-payments URL + token are configured in exactly one place.
//!
//! Inert until configured: if `LIT_PAYMENTS_INTERNAL_URL` is unset (or neither
//! `INTERNAL_SERVICE_TOKEN` nor `LIT_INTERNAL_SHARED_SECRET` is), `admit`
//! always allows and records nothing, and the write path returns
//! [`SpendingRulesError::NotConfigured`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use moka::future::Cache;
use serde::{Deserialize, Serialize};

use crate::core::v1::guards::request_meta::SpendingContext;
use crate::core::v1::helpers::api_status::ApiStatus;

/// SWR-ish freshness for the rules cache (kept short; only flagged keys pay it).
const RULES_CACHE_TTL: Duration = Duration::from_secs(30);
const RULES_CACHE_CAPACITY: u64 = 100_000;
/// Bound the hot-path cold-miss fetch so a slow lit-payments can't stall a call.
const FETCH_TIMEOUT: Duration = Duration::from_secs(2);
/// Per-second execution cost, mirrored from the Stripe charge rate so the
/// per-key counter tracks roughly what the account is billed.
const COST_PER_SECOND_CENTS: i64 = crate::stripe::COST_LIT_ACTION_PER_SECOND_CENTS;
/// Per-(key, IP) buckets are unbounded in principle (one per distinct client),
/// so once the map passes this size, entries idle longer than
/// [`IP_BUCKET_IDLE`] are swept on the next insert.
const IP_BUCKET_SWEEP_THRESHOLD: usize = 50_000;
const IP_BUCKET_IDLE: Duration = Duration::from_secs(3600);

/// The rules the gateway enforces for one key (subset of the lit-payments row).
/// `Default` is "no limits, enabled" — the same as an empty JSON object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    pub spend_cap_cents: Option<i64>,
    pub spend_window_seconds: Option<i64>,
    pub rate_limit_rps: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub max_concurrency: Option<i32>,
    #[serde(default)]
    pub ip_rate_limit_rps: Option<i32>,
    #[serde(default)]
    pub ip_rate_limit_burst: Option<i32>,
    /// `scheme://host[:port]` entries; a leading `*.` on the host matches any
    /// subdomain. `None`/empty = no origin check.
    #[serde(default)]
    pub allowed_origins: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            spend_cap_cents: None,
            spend_window_seconds: None,
            rate_limit_rps: None,
            rate_limit_burst: None,
            max_concurrency: None,
            ip_rate_limit_rps: None,
            ip_rate_limit_burst: None,
            allowed_origins: None,
            enabled: true,
        }
    }
}

/// Errors from the gateway → lit-payments write path (`set_rules` /
/// `delete_rules`). Mapped to HTTP by the account-management layer.
#[derive(Debug)]
pub enum SpendingRulesError {
    /// `LIT_PAYMENTS_INTERNAL_URL` / token not set on this node.
    NotConfigured,
    /// lit-payments rejected the rules (400 with its message).
    Rejected(String),
    /// lit-payments unreachable / 5xx / undecodable.
    Upstream(String),
}

impl std::fmt::Display for SpendingRulesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "spending rules are not configured on this node (LIT_PAYMENTS_INTERNAL_URL)"
            ),
            Self::Rejected(m) => write!(f, "spending rules rejected: {m}"),
            Self::Upstream(m) => write!(f, "spending rules service error: {m}"),
        }
    }
}

impl std::error::Error for SpendingRulesError {}

/// Shape of `GET /internal/spending-rules/<hash>` from lit-payments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesWithUsage {
    pub rules: RuleSet,
    pub usage: Option<UsageRow>,
}

/// Current rolling-window usage as stored by lit-payments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRow {
    pub spent_cents: i64,
    #[serde(default)]
    pub window_started_at: Option<String>,
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
    /// Keyed by `"<key hash>|<ip>"`; swept when large (see
    /// [`IP_BUCKET_SWEEP_THRESHOLD`]).
    ip_buckets: Mutex<HashMap<String, Bucket>>,
    concurrency: Mutex<HashMap<String, u32>>,
}

/// Shared, cheaply-clonable spending-rules enforcer. Managed in Rocket state.
#[derive(Clone)]
pub struct SpendingRulesState {
    inner: Arc<Inner>,
}

impl SpendingRulesState {
    /// Build from env. Enforcement is enabled only when
    /// `LIT_PAYMENTS_INTERNAL_URL` and a token (`INTERNAL_SERVICE_TOKEN`, or
    /// the existing `LIT_INTERNAL_SHARED_SECRET` already shared with
    /// lit-payments) are set; otherwise this is fully inert.
    pub fn from_env() -> Self {
        let env_trimmed = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let base_url =
            env_trimmed("LIT_PAYMENTS_INTERNAL_URL").map(|s| s.trim_end_matches('/').to_string());
        let token = env_trimmed("INTERNAL_SERVICE_TOKEN")
            .or_else(|| env_trimmed("LIT_INTERNAL_SHARED_SECRET"));

        let enabled = base_url.is_some() && token.is_some();
        if enabled {
            tracing::info!("spending_rules: enforcement enabled");
        } else {
            tracing::info!(
                "spending_rules: disabled (set LIT_PAYMENTS_INTERNAL_URL + INTERNAL_SERVICE_TOKEN \
                 or LIT_INTERNAL_SHARED_SECRET to enable)"
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
                ip_buckets: Mutex::new(HashMap::new()),
                concurrency: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Whether this node can reach lit-payments' spending-rules store.
    pub fn is_configured(&self) -> bool {
        self.inner.enabled
    }

    /// Enforce a flagged key's rules before execution. Returns an [`Admission`]
    /// the caller holds across execution (releasing any concurrency permit on
    /// drop) and calls [`Admission::record_spend`] on afterwards.
    ///
    /// `has_spending_rules` is the on-chain gate; pass it so we skip all work
    /// (and the lit-payments round trip) for keys without rules. `ctx` carries
    /// the request's `Origin` and client IP for the allowlist / per-IP checks.
    pub async fn admit(
        &self,
        api_key: &str,
        has_spending_rules: bool,
        ctx: &SpendingContext,
    ) -> Result<Admission, ApiStatus> {
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

        // Origin allowlist (cheapest check, and a rejected origin should not
        // consume rate-limit tokens the legitimate frontend needs).
        if let Some(allowed) = rules.allowed_origins.as_deref().filter(|a| !a.is_empty()) {
            match ctx.origin.as_deref() {
                Some(origin) if origin_allowed(origin, allowed) => {}
                Some(origin) => {
                    return Err(ApiStatus::forbidden(format!(
                        "origin {origin} is not allowed to use this API key"
                    )));
                }
                None => {
                    return Err(ApiStatus::forbidden(
                        "this API key may only be used from an allowed browser origin \
                         (missing Origin header)",
                    ));
                }
            }
        }

        // Per-key rate limit.
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

        // Per-(key, client IP) rate limit. With no resolvable client IP every
        // caller shares one bucket — never more permissive than the per-key
        // limit, matching the trust model in `guards::rate_limit`.
        if let (Some(rps), Some(burst)) = (rules.ip_rate_limit_rps, rules.ip_rate_limit_burst) {
            let ip = ctx
                .client_ip
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let ip_key = format!("{hash}|{ip}");
            let mut buckets = self.inner.ip_buckets.lock().unwrap();
            sweep_idle_buckets(&mut buckets, now);
            let bucket = buckets.entry(ip_key).or_insert(Bucket {
                tokens: burst as f64,
                last_refill: now,
            });
            if !bucket.try_take(now, rps as f64, burst as f64) {
                return Err(ApiStatus::too_many_requests(format!(
                    "rate limit exceeded for this API key from your address ({rps} rps per IP)"
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

    /// Drop the cached rules (and local counters) for a key so the next
    /// request sees a fresh row. Called after `set_rules` / `delete_rules`.
    pub async fn invalidate(&self, api_key_or_hash: &str) {
        let hash = key_hash_from_key_or_hash(api_key_or_hash);
        self.inner.rules_cache.invalidate(&hash).await;
        self.inner.usage.lock().unwrap().remove(&hash);
        self.inner.buckets.lock().unwrap().remove(&hash);
        self.inner.concurrency.lock().unwrap().remove(&hash);
        let prefix = format!("{hash}|");
        self.inner
            .ip_buckets
            .lock()
            .unwrap()
            .retain(|k, _| !k.starts_with(&prefix));
    }

    /// Current rules + usage for a key, straight from lit-payments (uncached —
    /// this is the account-owner read path, not the hot path). `Ok(None)` when
    /// the key has no rules.
    pub async fn get_rules(
        &self,
        api_key_or_hash: &str,
    ) -> Result<Option<RulesWithUsage>, SpendingRulesError> {
        if !self.inner.enabled {
            return Err(SpendingRulesError::NotConfigured);
        }
        let hash = key_hash_from_key_or_hash(api_key_or_hash);
        let url = format!("{}/internal/spending-rules/{}", self.inner.base_url, hash);
        let resp = self
            .inner
            .http
            .get(&url)
            .bearer_auth(&self.inner.token)
            .send()
            .await
            .map_err(|e| SpendingRulesError::Upstream(e.to_string()))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = check_upstream(resp).await?;
        resp.json::<RulesWithUsage>()
            .await
            .map(Some)
            .map_err(|e| SpendingRulesError::Upstream(format!("decode failed: {e}")))
    }

    /// Create/replace a key's rules in lit-payments. The caller (account
    /// management) is responsible for flipping the on-chain flag afterwards.
    pub async fn set_rules(
        &self,
        api_key_or_hash: &str,
        rules: &RuleSet,
    ) -> Result<RuleSet, SpendingRulesError> {
        if !self.inner.enabled {
            return Err(SpendingRulesError::NotConfigured);
        }
        let hash = key_hash_from_key_or_hash(api_key_or_hash);
        let url = format!("{}/internal/spending-rules/{}", self.inner.base_url, hash);
        let resp = self
            .inner
            .http
            .put(&url)
            .bearer_auth(&self.inner.token)
            .json(rules)
            .send()
            .await
            .map_err(|e| SpendingRulesError::Upstream(e.to_string()))?;
        let resp = check_upstream(resp).await?;
        let stored = resp
            .json::<RuleSet>()
            .await
            .map_err(|e| SpendingRulesError::Upstream(format!("decode failed: {e}")))?;
        self.invalidate(&hash).await;
        Ok(stored)
    }

    /// Delete a key's rules + usage counter in lit-payments. Returns whether a
    /// row existed.
    pub async fn delete_rules(&self, api_key_or_hash: &str) -> Result<bool, SpendingRulesError> {
        if !self.inner.enabled {
            return Err(SpendingRulesError::NotConfigured);
        }
        let hash = key_hash_from_key_or_hash(api_key_or_hash);
        let url = format!("{}/internal/spending-rules/{}", self.inner.base_url, hash);
        let resp = self
            .inner
            .http
            .delete(&url)
            .bearer_auth(&self.inner.token)
            .send()
            .await
            .map_err(|e| SpendingRulesError::Upstream(e.to_string()))?;
        let resp = check_upstream(resp).await?;
        #[derive(Deserialize)]
        struct Deleted {
            deleted: bool,
        }
        let body = resp
            .json::<Deleted>()
            .await
            .map_err(|e| SpendingRulesError::Upstream(format!("decode failed: {e}")))?;
        self.invalidate(&hash).await;
        Ok(body.deleted)
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

/// Map a lit-payments response to our error type: 4xx → `Rejected` with the
/// server's message, 5xx → `Upstream`.
async fn check_upstream(resp: reqwest::Response) -> Result<reqwest::Response, SpendingRulesError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    #[derive(Deserialize)]
    struct ErrBody {
        error: String,
    }
    let msg = match resp.json::<ErrBody>().await {
        Ok(b) => b.error,
        Err(_) => status.to_string(),
    };
    if status.is_client_error() {
        Err(SpendingRulesError::Rejected(msg))
    } else {
        Err(SpendingRulesError::Upstream(format!("{status}: {msg}")))
    }
}

/// Evict idle per-IP buckets once the map is large. O(n) but only runs when
/// the map has grown past the threshold, so amortised cost stays small.
fn sweep_idle_buckets(buckets: &mut HashMap<String, Bucket>, now: Instant) {
    if buckets.len() >= IP_BUCKET_SWEEP_THRESHOLD {
        buckets.retain(|_, b| now.duration_since(b.last_refill) < IP_BUCKET_IDLE);
    }
}

/// Does the request `Origin` match one of the allowlist entries?
///
/// Both sides are compared as `scheme://host[:port]` with scheme and host
/// lowercased (the browser sends them lowercased already). An entry whose host
/// starts with `*.` matches any subdomain (one or more labels) of the
/// remainder, on the same scheme and port, but not the bare apex.
pub fn origin_allowed(origin: &str, allowed: &[String]) -> bool {
    let Some(origin) = normalize_origin(origin) else {
        return false;
    };
    allowed.iter().any(|entry| {
        let Some(entry) = normalize_origin(entry) else {
            return false;
        };
        if let Some((scheme, wild_host)) = entry.split_once("://*.") {
            let Some((o_scheme, o_host)) = origin.split_once("://") else {
                return false;
            };
            if o_scheme != scheme {
                return false;
            }
            // Split host[:port] on both sides so the port must match exactly.
            let (o_h, o_p) = split_host_port(o_host);
            let (w_h, w_p) = split_host_port(wild_host);
            o_p == w_p && o_h.len() > w_h.len() && o_h.ends_with(&format!(".{w_h}"))
        } else {
            origin == entry
        }
    })
}

/// Lowercase scheme+host, strip a single trailing slash, reject anything with
/// a path/query (which a real `Origin` header never carries).
fn normalize_origin(raw: &str) -> Option<String> {
    let s = raw.trim().trim_end_matches('/');
    let (scheme, rest) = s.split_once("://")?;
    if rest.is_empty() || rest.contains(['/', '?', '#', ' ']) {
        return None;
    }
    Some(format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        rest.to_ascii_lowercase()
    ))
}

/// `host[:port]` → `(host, Option<port>)`; a bracketed IPv6 literal is kept whole.
fn split_host_port(s: &str) -> (&str, Option<&str>) {
    if s.starts_with('[') {
        return match s.rsplit_once("]:") {
            Some((h, p)) => (&s[..h.len() + 1], Some(p)),
            None => (s, None),
        };
    }
    match s.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h, Some(p)),
        _ => (s, None),
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

/// Same as [`key_hash`], but accepts an already-hashed 32-byte hex key (as
/// returned by `list_api_keys`) and passes it through canonicalised.
fn key_hash_from_key_or_hash(s: &str) -> String {
    let h = crate::utils::parse_with_hash::usage_api_key_to_hash(s);
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

    fn ctx(origin: Option<&str>, ip: Option<&str>) -> SpendingContext {
        SpendingContext {
            origin: origin.map(str::to_string),
            client_ip: ip.map(|s| s.parse().unwrap()),
        }
    }

    #[test]
    fn origin_exact_match_is_case_insensitive_and_port_sensitive() {
        let allowed = vec!["https://app.example.com".to_string()];
        assert!(origin_allowed("https://app.example.com", &allowed));
        assert!(origin_allowed("HTTPS://App.Example.COM/", &allowed));
        assert!(!origin_allowed("http://app.example.com", &allowed));
        assert!(!origin_allowed("https://app.example.com:8443", &allowed));
        assert!(!origin_allowed("https://evil.example.com", &allowed));
        assert!(!origin_allowed(
            "https://app.example.com.evil.com",
            &allowed
        ));
        assert!(!origin_allowed("null", &allowed));
    }

    #[test]
    fn origin_wildcard_matches_subdomains_not_apex() {
        let allowed = vec!["https://*.example.com".to_string()];
        assert!(origin_allowed("https://app.example.com", &allowed));
        assert!(origin_allowed("https://a.b.example.com", &allowed));
        assert!(!origin_allowed("https://example.com", &allowed));
        assert!(!origin_allowed("https://notexample.com", &allowed));
        assert!(!origin_allowed("http://app.example.com", &allowed));
        assert!(!origin_allowed("https://app.example.com:3000", &allowed));
        let with_port = vec!["http://*.localhost:3000".to_string()];
        assert!(origin_allowed("http://dev.localhost:3000", &with_port));
        assert!(!origin_allowed("http://dev.localhost", &with_port));
    }

    #[test]
    fn split_host_port_handles_ipv6() {
        assert_eq!(split_host_port("[::1]:3000"), ("[::1]", Some("3000")));
        assert_eq!(split_host_port("[::1]"), ("[::1]", None));
        assert_eq!(split_host_port("localhost:80"), ("localhost", Some("80")));
        assert_eq!(split_host_port("localhost"), ("localhost", None));
    }

    #[test]
    fn sweep_only_runs_past_threshold_and_keeps_active() {
        let now = Instant::now();
        let mut m = HashMap::new();
        m.insert(
            "stale".to_string(),
            Bucket {
                tokens: 0.0,
                last_refill: now - IP_BUCKET_IDLE * 2,
            },
        );
        sweep_idle_buckets(&mut m, now);
        assert_eq!(m.len(), 1, "below threshold: nothing swept");
        for i in 0..IP_BUCKET_SWEEP_THRESHOLD {
            m.insert(
                format!("live{i}"),
                Bucket {
                    tokens: 1.0,
                    last_refill: now,
                },
            );
        }
        sweep_idle_buckets(&mut m, now);
        assert!(!m.contains_key("stale"));
        assert_eq!(m.len(), IP_BUCKET_SWEEP_THRESHOLD);
    }

    /// A state that is "configured" (so `admit` runs) but whose rules cache is
    /// pre-seeded, so no network call happens.
    async fn seeded_state(rules: RuleSet) -> (SpendingRulesState, &'static str) {
        let key = "test-usage-key";
        let state = SpendingRulesState {
            inner: Arc::new(Inner {
                enabled: true,
                base_url: "http://127.0.0.1:1".into(),
                token: "t".into(),
                http: reqwest::Client::new(),
                rules_cache: Cache::builder().build(),
                usage: Mutex::new(HashMap::new()),
                buckets: Mutex::new(HashMap::new()),
                ip_buckets: Mutex::new(HashMap::new()),
                concurrency: Mutex::new(HashMap::new()),
            }),
        };
        state
            .inner
            .rules_cache
            .insert(key_hash(key), Some(Arc::new(rules)))
            .await;
        (state, key)
    }

    #[tokio::test]
    async fn admit_enforces_origin_allowlist() {
        let (state, key) = seeded_state(RuleSet {
            allowed_origins: Some(vec!["https://app.example.com".into()]),
            ..Default::default()
        })
        .await;
        assert!(
            state
                .admit(key, true, &ctx(Some("https://app.example.com"), None))
                .await
                .is_ok()
        );
        let denied = state
            .admit(key, true, &ctx(Some("https://evil.com"), None))
            .await;
        assert_eq!(
            denied.err().map(|e| e.status),
            Some(rocket::http::Status::Forbidden)
        );
        let missing = state.admit(key, true, &ctx(None, None)).await;
        assert_eq!(
            missing.err().map(|e| e.status),
            Some(rocket::http::Status::Forbidden)
        );
        // Unflagged keys skip everything, even with a bad origin.
        assert!(
            state
                .admit(key, false, &ctx(Some("https://evil.com"), None))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn admit_enforces_per_ip_bucket_independently() {
        let (state, key) = seeded_state(RuleSet {
            ip_rate_limit_rps: Some(1),
            ip_rate_limit_burst: Some(1),
            ..Default::default()
        })
        .await;
        assert!(
            state
                .admit(key, true, &ctx(None, Some("10.0.0.1")))
                .await
                .is_ok()
        );
        let throttled = state.admit(key, true, &ctx(None, Some("10.0.0.1"))).await;
        assert_eq!(
            throttled.err().map(|e| e.status),
            Some(rocket::http::Status::TooManyRequests)
        );
        // A different client still has its own burst.
        assert!(
            state
                .admit(key, true, &ctx(None, Some("10.0.0.2")))
                .await
                .is_ok()
        );
        // invalidate() clears the per-IP buckets for the key.
        state.invalidate(key).await;
        assert!(
            state
                .admit(key, true, &ctx(None, Some("10.0.0.1")))
                .await
                .is_ok()
        );
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
