/// Stripe billing integration using customer balance as a credit ledger.
///
/// Credits flow:
///   • Funding:   PaymentIntent succeeds → create customer balance transaction (amount = -cents)
///                This makes the balance more negative = more credits available.
///   • Charging:  Before each API call we check `balance + cost <= 0`; if so we create a
///                positive balance transaction (depletes credits).
///
/// Customer identity: the Stripe customer is keyed by the wallet address derived from the API key
/// (stored in customer metadata as `wallet_address`).
///
/// The raw Stripe HTTP client + customer/balance/reporting primitives live in
/// `lit-billing-core` so the same identity model is shared with `lit-payments`.
/// This module wraps the core client with the in-process caches and the
/// charge / PaymentIntent flows specific to the API server.
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use lit_billing_core::StripeClient;
use moka::future::Cache;
use tracing::instrument;

// Re-export the bits of lit-billing-core that out-of-crate callers (bin/stripe_report.rs,
// other modules in lit-api-server) reference via `lit_api_server::stripe::*`.
pub use lit_billing_core::format::{cents_to_display, unix_to_utc_date};
pub use lit_billing_core::reporting::{
    ReportBalanceTx, ReportCustomer, ReportRow, aggregate_report_rows,
};

/// Cost constants in US cents.
pub const COST_MANAGEMENT_CENTS: i64 = 1; // $0.01
pub const COST_LIT_ACTION_PER_SECOND_CENTS: i64 = 1; // $0.01 per second of execution
/// Minimum top-up (500 cents = $5.00).
pub const MIN_TOPUP_CENTS: i64 = 500;

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Why a billing check or charge failed.
///
/// Guards map each variant to a distinct HTTP status so callers can tell an
/// auth problem (401) from a funding problem (402) from a billing-infra
/// problem (503). Previously all three surfaced as `402 Payment Required`,
/// which told developers with a typo'd key to add funds.
#[derive(Debug)]
pub enum BillingError {
    /// The API key does not resolve to any on-chain account → 401.
    InvalidApiKey,
    /// The account exists but cannot cover the operation → 402.
    InsufficientCredits {
        /// Credits available, in cents (≥ 0; Stripe-balance sign already flipped).
        available_cents: i64,
        /// Cents this operation needs.
        required_cents: i64,
    },
    /// Chain RPC or Stripe failed — not the caller's fault → 503.
    Unavailable(anyhow::Error),
}

impl std::fmt::Display for BillingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BillingError::InvalidApiKey => {
                write!(f, "API key does not resolve to any account")
            }
            BillingError::InsufficientCredits {
                available_cents,
                required_cents,
            } => write!(
                f,
                "insufficient credits: have {}, need {}",
                cents_to_display(*available_cents),
                cents_to_display(*required_cents)
            ),
            BillingError::Unavailable(e) => write!(f, "billing unavailable: {e}"),
        }
    }
}

impl std::error::Error for BillingError {}

// ─── Observability ──────────────────────────────────────────────────────────
//
// Every charge is surfaced to the observability pipeline (CPL-329) so that spend
// can be broken down by source and by payer. Charges are emitted both as metrics
// (for dashboards/alerting) and as a per-payment structured `tracing` event (for
// per-payment audit in the logs/traces pipeline).
//
// The billing wallet address is a first-class metric label (`wallet_address`) so
// spend can be sliced per account directly in the metric backend. Note this makes
// the time-series cardinality scale with the number of distinct billing wallets.

/// Why a charge was incurred. Surfaced as the `reason` label on billing metrics
/// and the `billing_reason` field on per-payment events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BillingReason {
    /// A management / state-change API call (flat per-call charge).
    Management,
    /// Lit Action execution time (per-second charge).
    LitAction,
}

impl BillingReason {
    pub fn as_str(self) -> &'static str {
        match self {
            BillingReason::Management => "management",
            BillingReason::LitAction => "lit_action",
        }
    }
}

/// Charge outcomes, used as the `outcome` label on `billing.charge.count`.
const OUTCOME_ACCEPTED: &str = "accepted";
const OUTCOME_INSUFFICIENT_CREDITS: &str = "insufficient_credits";

/// Emit observability for a charge decision (credit deducted, or rejected).
///
/// `cost_cents` is the amount the charge represented; for a rejected charge it is
/// the amount we *attempted* to charge. Only accepted charges contribute to the
/// `billing.charge.amount_cents` total.
fn record_billing_event(
    reason: BillingReason,
    wallet_address: &str,
    cost_cents: i64,
    outcome: &'static str,
) {
    metrics::counter!(
        "billing.charge.count",
        "reason" => reason.as_str(),
        "outcome" => outcome,
        "wallet_address" => wallet_address.to_owned(),
    )
    .increment(1);

    if outcome == OUTCOME_ACCEPTED {
        metrics::counter!(
            "billing.charge.amount_cents",
            "reason" => reason.as_str(),
            "wallet_address" => wallet_address.to_owned(),
        )
        .increment(cost_cents.max(0) as u64);
    }

    // Per-payment record for per-payment audit in the logs/traces pipeline.
    // Logged at info so it reaches the pipeline without enabling debug logging.
    tracing::info!(
        billing_wallet_address = wallet_address,
        billing_reason = reason.as_str(),
        billing_amount_cents = cost_cents,
        billing_outcome = outcome,
        "billing charge event",
    );
}

// ─── State ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct StripeState {
    pub publishable_key: String,
    client: StripeClient,
    /// wallet_address → Stripe customer ID cache (10-min idle timeout).
    /// Avoids duplicate customer creation caused by Stripe Search API indexing lag.
    /// Uses `time_to_idle` so frequently accessed entries stay warm.
    customer_cache: Cache<String, String>,
    /// wallet_address → recent "no customer found in Search" marker (5-sec TTL).
    /// Rate-limits the guards' find-only lookup: without it, every guarded
    /// request for a wallet with no (indexed) customer would issue a Stripe
    /// Search call, so one unfunded key under load could burn through Stripe's
    /// search rate limit. A 5-second cooldown bounds that to ~0.2 searches/sec
    /// per wallet while only delaying post-funding recovery by ≤5s — a bounded
    /// negative cache, unlike the permanent poisoning this replaces (#555).
    customer_search_miss: Cache<String, ()>,
    /// api_key → billing wallet address cache.
    /// Resolves both master and usage API keys to the account's billing wallet address
    /// (the wallet used to identify the Stripe customer) via the on-chain
    /// `allApiKeyHashesToMaster` mapping, avoiding a contract call per charge.
    wallet_cache: Cache<String, String>,
    /// customer_id → credit balance cache (10-min TTL).
    /// Avoids a Stripe API call on every charge; stale reads may allow some
    /// overcharging which is acceptable per CPL-246.
    balance_cache: Cache<String, i64>,
    /// Guards against thundering-herd background refreshes and acts as a cooldown.
    /// When present for a customer_id, no new refresh is spawned.  Entries live
    /// for 60 seconds, so each customer triggers at most one Stripe GET per minute
    /// regardless of request rate.
    balance_refresh_in_flight: Cache<String, ()>,
    /// Credits (in cents) granted to every new account, from `STARTER_CREDITS_CENTS`.
    /// 0 (the default) disables the grant entirely.
    starter_credits_cents: i64,
}

/// Env var that opts out of the local-development billing requirement (CPL-330).
/// When set to a truthy value (`1`/`true`/`yes`/`on`) the server runs
/// payment-free even on a local (non-production) build.
pub const DISABLE_BILLING_ENV: &str = "LIT_DISABLE_BILLING";

/// Read the Stripe keys from the environment.  Returns `None` if either is
/// absent or empty (billing not configured).
///
/// Keys are trimmed of surrounding whitespace: a trailing newline (common when
/// a key is piped in via `export FOO=$(cat secret)`) would otherwise corrupt the
/// HTTP `Authorization` header and the prefix checks below.
fn stripe_keys_from_env() -> Option<(String, String)> {
    let secret_key = std::env::var("STRIPE_SECRET_KEY").ok()?.trim().to_string();
    let publishable_key = std::env::var("STRIPE_PUBLISHABLE_KEY")
        .ok()?
        .trim()
        .to_string();
    if secret_key.is_empty() || publishable_key.is_empty() {
        return None;
    }
    Some((secret_key, publishable_key))
}

/// Whether billing has been explicitly disabled via [`DISABLE_BILLING_ENV`].
fn billing_disabled() -> bool {
    std::env::var(DISABLE_BILLING_ENV)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// A Stripe key is "live mode" when it starts with a live-mode prefix
/// (`sk_live_`, `rk_live_`, `pk_live_`).  Used to keep live keys off
/// local (non-production) builds so a dev machine can't charge real cards.
fn is_live_key(key: &str) -> bool {
    key.starts_with("sk_live_") || key.starts_with("rk_live_") || key.starts_with("pk_live_")
}

/// Validate that the supplied keys are genuine *test-mode* Stripe keys of the
/// correct role, for the local (non-production) billing policy (CPL-330).
///
/// A denylist of live prefixes is not enough on its own:
///   - An arbitrary non-live string (e.g. `not-a-stripe-key`) would pass a
///     "not live" check, start the server logging "test mode", and only fail
///     when the first request hits Stripe — defeating the requirement to
///     configure a real test account.
///   - A *secret* key (`sk_test_…`/`rk_test_…`) placed in `STRIPE_PUBLISHABLE_KEY`
///     would pass and then be served to unauthenticated clients via
///     `GET /billing/stripe_config`, leaking secret material.
///
/// So we require positive, role-correct test prefixes:
///   - secret:      `sk_test_…` or `rk_test_…`
///   - publishable: `pk_test_…`
///
/// Pure and feature-independent so it is unit-tested under the default build
/// (CI's `--all-features` run enables `production`, which skips the local policy
/// path entirely).
fn validate_local_test_keys(secret_key: &str, publishable_key: &str) -> Result<()> {
    // Friendlier, more specific message for the live-key footgun.
    if is_live_key(secret_key) || is_live_key(publishable_key) {
        anyhow::bail!(
            "Refusing to start: a LIVE Stripe key was supplied on a non-production build. Local \
             runs must use TEST keys (sk_test_…/rk_test_…/pk_test_…) so a dev machine can't \
             charge real cards.\n  Use a test key, or set {DISABLE_BILLING_ENV}=true to run \
             payment-free."
        );
    }
    if !(secret_key.starts_with("sk_test_") || secret_key.starts_with("rk_test_")) {
        anyhow::bail!(
            "STRIPE_SECRET_KEY is not a TEST secret key. Local runs require a real test Stripe \
             account (CPL-330): the secret must start with sk_test_ or rk_test_.\n  Set a valid \
             test key, or set {DISABLE_BILLING_ENV}=true to run payment-free."
        );
    }
    if !publishable_key.starts_with("pk_test_") {
        anyhow::bail!(
            "STRIPE_PUBLISHABLE_KEY is not a TEST publishable key. It must start with pk_test_ \
             (a secret key here would be served to unauthenticated clients via \
             /billing/stripe_config).\n  Set a valid pk_test_ key, or set {DISABLE_BILLING_ENV}=true \
             to run payment-free."
        );
    }
    Ok(())
}

/// Build `StripeState` from the given keys, wiring up the in-process caches.
fn build_state(secret_key: String, publishable_key: String) -> Result<Arc<StripeState>> {
    let client = StripeClient::new(secret_key)?;
    let customer_cache = Cache::builder()
        .max_capacity(10_000)
        .time_to_idle(Duration::from_secs(600)) // 10 minutes
        .build();
    let customer_search_miss = Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(5)) // short: bounds Search rate, not a poison
        .build();
    let wallet_cache = Cache::builder()
        .max_capacity(10_000)
        .time_to_idle(Duration::from_secs(3600))
        .build();
    let balance_cache = Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(600)) // 10 minutes hard TTL
        .build();
    let balance_refresh_in_flight = Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(60)) // cooldown: max 1 refresh per customer per minute
        .build();
    let starter_credits_cents = read_starter_credits_env();
    if starter_credits_cents > 0 {
        tracing::info!(
            starter_credits_cents,
            "stripe: starter credits enabled for new accounts"
        );
    }
    Ok(Arc::new(StripeState {
        publishable_key,
        client,
        customer_cache,
        customer_search_miss,
        wallet_cache,
        balance_cache,
        balance_refresh_in_flight,
        starter_credits_cents,
    }))
}

/// Build `StripeState` directly from the environment with no policy enforcement.
///
/// Returns `None` when the keys are absent/empty, and logs+returns `None` when
/// the client fails to build.  This is the historical `init()` behaviour and is
/// used by out-of-band tooling (e.g. `stripe_report`) that must work against
/// whatever account is configured — including live keys — regardless of build.
pub fn from_env() -> Option<Arc<StripeState>> {
    let (secret_key, publishable_key) = stripe_keys_from_env()?;
    match build_state(secret_key, publishable_key) {
        Ok(state) => Some(state),
        Err(e) => {
            tracing::error!("stripe: failed to build HTTP client: {e}");
            None
        }
    }
}

/// Initialise Stripe billing, applying the local-development billing policy.
///
/// **Production builds** (`--features production`): billing is enabled when the
/// `STRIPE_SECRET_KEY` / `STRIPE_PUBLISHABLE_KEY` env vars are present and
/// disabled (returns `Ok(None)`) when they are absent — historical behaviour.
/// Production legitimately uses live keys, so key mode is not constrained.
///
/// **Local / non-production builds**: by default a *test* Stripe account MUST be
/// configured so local runs exercise the real billing path instead of silently
/// dropping into payment-free mode (CPL-330).  The server refuses to start when:
///   - the keys are missing, or
///   - the keys are not role-correct *test* keys: the secret must be
///     `sk_test_…`/`rk_test_…` and the publishable must be `pk_test_…`.  This
///     rejects live keys (so a dev machine can't charge real cards), arbitrary
///     non-Stripe strings, and a secret key mistakenly placed in the publishable
///     slot (which would leak via `GET /billing/stripe_config`).
///
/// Set `LIT_DISABLE_BILLING=true` to opt out and run payment-free.
pub fn init() -> Result<Option<Arc<StripeState>>> {
    // Production keeps the historical "use billing iff configured" behaviour and
    // does not constrain key mode (production legitimately uses live keys).
    if cfg!(feature = "production") {
        let state = from_env();
        if state.is_some() {
            tracing::info!("stripe: billing enabled");
        }
        return Ok(state);
    }

    // ── Local / non-production policy (CPL-330) ─────────────────────────────
    if billing_disabled() {
        tracing::warn!(
            "{DISABLE_BILLING_ENV} is set — running payment-free. Local billing enforcement disabled."
        );
        return Ok(None);
    }

    let Some((secret_key, publishable_key)) = stripe_keys_from_env() else {
        anyhow::bail!(
            "Stripe billing is not configured. Local runs require a TEST Stripe account so the \
             billing path is exercised (CPL-330).\n  \
             Set STRIPE_SECRET_KEY (sk_test_… or rk_test_…) and STRIPE_PUBLISHABLE_KEY (pk_test_…),\n  \
             or set {DISABLE_BILLING_ENV}=true to run payment-free."
        );
    };

    validate_local_test_keys(&secret_key, &publishable_key)?;

    let state = build_state(secret_key, publishable_key)?;
    tracing::info!("stripe: billing enabled (test mode)");
    Ok(Some(state))
}

// ─── Startup key validation ────────────────────────────────────────────────
//
// `init()` only checks that the keys are *present* and well-formed. A key that
// is present but revoked, or for the wrong Stripe environment (a live key in a
// test deployment, or vice versa), passes `init()` and then fails every real
// billing request — a silent failure discovered only when the first customer
// is charged. `validate_key()` closes that gap by exercising the key against
// Stripe once at boot.

/// Outcome of the startup key check ([`validate_key`]).
///
/// The split between [`AuthFailed`](KeyCheck::AuthFailed) and
/// [`Unavailable`](KeyCheck::Unavailable) is the whole point: a bad *key* is a
/// fatal misconfiguration the operator must fix, whereas a Stripe *outage* says
/// nothing about the key and must not stop the server from booting.
#[derive(Debug)]
pub enum KeyCheck {
    /// `GET /v1/balance` returned success — the key authenticates against Stripe.
    Ok,
    /// Stripe rejected the credentials (HTTP 401/403). The key is revoked,
    /// malformed, or for the wrong environment. Fatal: the caller should refuse
    /// to start rather than run with billing that can never succeed.
    AuthFailed(String),
    /// Stripe could not be reached (5xx, timeout, transport error) or returned an
    /// otherwise non-auth error. Inconclusive and not the key's fault: the caller
    /// should keep billing enabled and let the first real billing request retry.
    Unavailable(String),
}

/// Validate the configured Stripe secret key at startup via `GET /v1/balance`.
///
/// `/v1/balance` is the cheapest authenticated read Stripe offers: it touches no
/// customer data and its only precondition is a working key, which makes it a
/// clean liveness probe for the credentials.
///
/// Returns a [`KeyCheck`]: callers treat [`KeyCheck::AuthFailed`] as fatal and
/// [`KeyCheck::Unavailable`] as a soft warning (billing stays enabled). This is
/// a free function rather than a method so the auth/availability classification
/// in [`classify_key_check`] can be unit-tested without a live Stripe.
pub async fn validate_key(state: &StripeState) -> KeyCheck {
    match state.client.get("balance", &[]).await {
        Ok(_) => KeyCheck::Ok,
        Err(e) => classify_key_check(&e),
    }
}

/// Map a `GET /v1/balance` error to a [`KeyCheck`].
///
/// Only a definitive auth rejection (HTTP 401/403) is treated as fatal. Every
/// other error — 5xx, rate limits, timeouts, transport failures, malformed JSON
/// — is availability, not a verdict on the key, so we degrade gracefully. Erring
/// toward `Unavailable` means a Stripe blip can never take down our own startup.
fn classify_key_check(e: &anyhow::Error) -> KeyCheck {
    if let Some(se) = e
        .chain()
        .find_map(|cause| cause.downcast_ref::<lit_billing_core::StripeError>())
    {
        let code = se.status.as_u16();
        if code == 401 || code == 403 {
            return KeyCheck::AuthFailed(se.to_string());
        }
        return KeyCheck::Unavailable(se.to_string());
    }
    // No StripeError on the chain → transport/timeout/JSON error from reqwest,
    // never an authentication signal.
    KeyCheck::Unavailable(e.to_string())
}

/// Parse `STARTER_CREDITS_CENTS`. Unset, empty, unparseable, or negative → 0 (off).
fn read_starter_credits_env() -> i64 {
    let Ok(raw) = std::env::var("STARTER_CREDITS_CENTS") else {
        return 0;
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return 0;
    }
    match raw.parse::<i64>() {
        Ok(v) if v >= 0 => v,
        Ok(v) => {
            tracing::warn!("STARTER_CREDITS_CENTS is negative ({v}); starter credits disabled");
            0
        }
        Err(e) => {
            tracing::warn!("STARTER_CREDITS_CENTS unparseable ({e}); starter credits disabled");
            0
        }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Compute a non-sensitive cache key from an account identity string.
///
/// Accepts either a raw API key (hashed via keccak256) or a precomputed
/// 0x-prefixed 32-byte hex hash (used by ChainSecured callers, whose on-chain
/// identity is `keccak256(walletAddress)`). Both forms collapse to the same
/// U256 cache key, so a raw key and its hash share a cache entry.
///
/// Using the hash means no secret material is held in the cache's key set —
/// avoids leaking raw API keys via memory dumps, debug tooling, or telemetry.
fn cache_key(key_or_hash: &str) -> String {
    crate::utils::parse_with_hash::usage_api_key_to_hash(key_or_hash).to_string()
}

/// Remove an API key from the wallet address cache.
///
/// Call this when a usage API key is deleted so that stale mappings are not served.
pub async fn invalidate_wallet_cache(api_key: &str, state: &StripeState) {
    state.wallet_cache.invalidate(&cache_key(api_key)).await;
}

impl StripeState {
    /// Drop the cached Stripe balance for a customer. Called by the internal
    /// `/internal/invalidate_balance_cache` endpoint after `lit-payments`
    /// applies a sync auto-top-up credit, so the next read goes back to
    /// Stripe rather than serving the stale pre-credit balance from the
    /// 10-minute TTL cache. Same primitive used internally by
    /// `confirm_payment_intent` after a manual top-up.
    pub async fn invalidate_balance_cache(&self, customer_id: &str) {
        self.balance_cache.invalidate(customer_id).await;
    }

    /// Entry counts of the billing caches, for `/get_system_stats`.
    /// With `flush`, moka's internal buffers are flushed first so the counts
    /// reflect completed inserts/evictions rather than lagging them — the
    /// caller throttles flushes because the endpoint is public.
    pub async fn cache_entry_counts(&self, flush: bool) -> [(&'static str, u64); 3] {
        if flush {
            self.customer_cache.run_pending_tasks().await;
            self.wallet_cache.run_pending_tasks().await;
            self.balance_cache.run_pending_tasks().await;
        }
        [
            ("billing_customer", self.customer_cache.entry_count()),
            ("billing_wallet", self.wallet_cache.entry_count()),
            ("billing_balance", self.balance_cache.entry_count()),
        ]
    }
}

/// Resolve any account identity to its billing wallet address.
///
/// Accepts a raw API key (master or usage) or — for ChainSecured callers — a
/// precomputed 0x-prefixed 32-byte hex hash (the wallet-derived
/// `keccak256(walletAddress)`). Uses the on-chain `allApiKeyHashesToMaster`
/// mapping so that usage API keys resolve to the same wallet (and therefore
/// same Stripe customer) as their parent account key. The billing wallet is
/// set at account creation and preserved across conversion to ChainSecured,
/// so charges keep hitting the same Stripe customer after the admin wallet
/// rotates (CPL-313). Legacy accounts without a billing wallet fall back to
/// the admin wallet on-chain. Results are cached for 1 hour.
///
/// The cache is keyed by the keccak256 hash of the input (not the raw key)
/// to avoid holding secret material in memory.
#[instrument(name = "stripe::resolve_wallet_address", skip_all, err)]
pub async fn resolve_wallet_address(api_key: &str, state: &StripeState) -> Result<String> {
    let key = cache_key(api_key);
    tracing::debug!("stripe::resolve_wallet_address: looking up wallet");
    let result = state
        .wallet_cache
        .try_get_with(key, async {
            tracing::debug!("stripe::resolve_wallet_address: cache miss, calling contract");
            crate::accounts::get_billing_wallet_address(api_key).await
        })
        .await
        // Moka wraps errors in `Arc`, which can't be unwrapped into the owned
        // error. Re-wrapping as a string would erase the typed `UnknownApiKey`
        // that guards rely on to answer 401 instead of 402 — so reconstruct it.
        .map_err(|e: Arc<anyhow::Error>| {
            if e.downcast_ref::<crate::accounts::UnknownApiKey>().is_some() {
                anyhow::Error::new(crate::accounts::UnknownApiKey)
            } else {
                anyhow::anyhow!("{e}")
            }
        });
    tracing::debug!(
        success = result.is_ok(),
        "stripe::resolve_wallet_address: done"
    );
    result
}

/// Find the Stripe customer for this wallet address, creating one if none exists.
///
/// Only for paths where creating a customer is legitimate (account creation,
/// top-ups, email registration, balance display). The billing guards
/// (`check_credit` / `charge`) must use [`find_customer_by_wallet`] instead:
/// the underlying lookup is Stripe Search, which is eventually consistent, so
/// creating on a search miss can duplicate a freshly funded customer (#555).
///
/// Results are cached in memory to avoid duplicate customer creation caused by
/// Stripe Search API indexing lag (newly created customers may not appear in
/// search results for several seconds).
///
/// Uses `try_get_with` to coalesce concurrent requests for the same wallet,
/// preventing duplicate Stripe customer creation under concurrent load.
#[instrument(name = "stripe::get_customer_by_wallet", skip_all, err)]
pub async fn get_customer_by_wallet(wallet_address: &str, state: &StripeState) -> Result<String> {
    tracing::debug!(
        wallet_address,
        "stripe::get_customer_by_wallet: looking up customer"
    );
    let state = state.clone();
    let wallet = wallet_address.to_string();
    state
        .customer_cache
        .try_get_with(wallet.clone(), async {
            lit_billing_core::customer::find_or_create_by_wallet(&state.client, &wallet).await
        })
        .await
        .map_err(|e: Arc<anyhow::Error>| anyhow::anyhow!("{e}"))
}

/// Find the Stripe customer for this wallet address WITHOUT creating one.
///
/// Billing-guard variant of [`get_customer_by_wallet`] (#555): Stripe Search
/// is eventually consistent (a freshly created customer can be invisible for
/// tens of seconds), so a guard that creates on a search miss can mint a
/// duplicate zero-credit customer for a wallet whose funded customer just
/// isn't indexed yet — and caching that duplicate turns a transient index lag
/// into a permanent 402. Here a miss is simply `Ok(None)`: nothing is created
/// and nothing is cached, so the next request re-checks Stripe and heals as
/// soon as the index catches up.
///
/// Positive results go into (and come from) the same `customer_cache` that
/// `get_customer_by_wallet` uses, so a customer created eagerly at account
/// creation is found here without touching Search at all.
#[instrument(name = "stripe::find_customer_by_wallet", skip_all, err)]
pub async fn find_customer_by_wallet(
    wallet_address: &str,
    state: &StripeState,
) -> Result<Option<String>> {
    if let Some(id) = state.customer_cache.get(wallet_address).await {
        return Ok(Some(id));
    }
    // Cooldown: a recent search already came back empty. Skip Stripe rather
    // than issuing a Search per guarded request (rate-limit protection); the
    // 5-second TTL keeps the "not found" answer bounded, not permanent.
    if state
        .customer_search_miss
        .get(wallet_address)
        .await
        .is_some()
    {
        return Ok(None);
    }
    let found = lit_billing_core::customer::find_by_wallet(&state.client, wallet_address).await?;
    match &found {
        Some(id) => {
            state
                .customer_cache
                .insert(wallet_address.to_string(), id.clone())
                .await;
        }
        None => {
            state
                .customer_search_miss
                .insert(wallet_address.to_string(), ())
                .await;
        }
    }
    Ok(found)
}

/// Return the current credit balance in cents (≤ 0 means credits available; the Stripe
/// balance field is negative when the customer has a credit).
///
/// Uses a stale-while-revalidate strategy: if a cached value exists it is returned
/// immediately *and* a single background task is spawned to refresh the cache from
/// Stripe (deduplicated via `balance_refresh_in_flight`).
/// On a cache miss the fetch is performed inline (the caller waits).  This keeps
/// the hot-path fast while ensuring the cache converges toward the true balance.
pub async fn get_credit_balance(customer_id: &str, state: &StripeState) -> Result<i64> {
    let cid = customer_id.to_string();

    if let Some(cached) = state.balance_cache.get(&cid).await {
        // Spawn a background refresh only if one is not already in flight.
        if state.balance_refresh_in_flight.get(&cid).await.is_none() {
            state
                .balance_refresh_in_flight
                .insert(cid.clone(), ())
                .await;
            let state = state.clone();
            let cid2 = cid.clone();
            tokio::spawn(async move {
                match lit_billing_core::balance::fetch(&state.client, &cid2).await {
                    Ok(fetched) => {
                        let current = state.balance_cache.get(&cid2).await;
                        if should_update_balance_cache(current, fetched) {
                            state.balance_cache.insert(cid2.clone(), fetched).await;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("stripe: background balance refresh failed for {cid2}: {e}")
                    }
                }
                // Do NOT invalidate balance_refresh_in_flight here.  The 60-second
                // TTL acts as a cooldown so each customer triggers at most one
                // Stripe GET per minute, even under sustained traffic.
            });
        }
        return Ok(cached);
    }

    // Cache miss — fetch inline with request coalescing so concurrent misses
    // for the same customer produce only a single Stripe GET.
    let state2 = state.clone();
    let cid2 = cid.clone();
    state
        .balance_cache
        .try_get_with(cid, async move {
            lit_billing_core::balance::fetch(&state2.client, &cid2).await
        })
        .await
        .map_err(|e: Arc<anyhow::Error>| anyhow::anyhow!("{e}"))
}

/// Decide whether a background-refreshed balance should replace the cached value.
///
/// Returns `true` when:
/// - There is no cached value (cache was evicted or invalidated).
/// - The fetched balance is less negative (higher) than the cached value, meaning
///   Stripe processed additional charges we didn't know about.
///
/// Returns `false` when the fetched balance is more negative (lower) than the
/// cached value.  This preserves optimistic decrements made by `charge()`: if
/// we wrote -999 but Stripe still shows -1000 (the fire-and-forget hasn't landed),
/// we keep -999.  Top-ups are handled by explicit `balance_cache.invalidate()` in
/// `confirm_payment_and_credit`, not by this refresh.
fn should_update_balance_cache(cached: Option<i64>, fetched: i64) -> bool {
    match cached {
        Some(c) => fetched > c,
        None => true,
    }
}

/// Resolve the billing wallet, classifying failures for status mapping.
async fn resolve_wallet_classified(
    api_key: &str,
    state: &StripeState,
) -> std::result::Result<String, BillingError> {
    resolve_wallet_address(api_key, state).await.map_err(|e| {
        // `AccountDoesNotExist` arrives as a decoded contract-revert string
        // (see accounts::get_billing_wallet_address) — same meaning as the
        // typed zero-address case: this key maps to no account.
        if e.downcast_ref::<crate::accounts::UnknownApiKey>().is_some()
            || e.to_string().contains("AccountDoesNotExist")
        {
            BillingError::InvalidApiKey
        } else {
            BillingError::Unavailable(e)
        }
    })
}

/// Verify the account can cover `required_cents` WITHOUT charging.
///
/// Used by request guards so nothing is deducted before the request body has
/// been validated and the handler has succeeded — the actual charge happens
/// post-response (management ops) or during execution (lit actions). An
/// insufficient balance is recorded as a rejected-charge billing event so the
/// existing metrics keep seeing guard rejections.
pub async fn check_credit(
    api_key: &str,
    required_cents: i64,
    reason: BillingReason,
    state: &StripeState,
) -> std::result::Result<(), BillingError> {
    let wallet = resolve_wallet_classified(api_key, state).await?;
    // No customer means no credits — answer 402 without creating one. Creating
    // here would race Stripe Search's indexing lag and could shadow a freshly
    // funded customer with a cached zero-credit duplicate (#555). A wallet
    // whose customer just isn't indexed yet gets a *retryable* 402 that heals
    // by itself within Stripe's indexing window.
    let Some(customer_id) = find_customer_by_wallet(&wallet, state)
        .await
        .map_err(BillingError::Unavailable)?
    else {
        record_billing_event(
            reason,
            &wallet,
            required_cents,
            OUTCOME_INSUFFICIENT_CREDITS,
        );
        return Err(BillingError::InsufficientCredits {
            available_cents: 0,
            required_cents,
        });
    };
    let balance = get_credit_balance(&customer_id, state)
        .await
        .map_err(BillingError::Unavailable)?;
    if balance + required_cents > 0 {
        record_billing_event(
            reason,
            &wallet,
            required_cents,
            OUTCOME_INSUFFICIENT_CREDITS,
        );
        return Err(BillingError::InsufficientCredits {
            available_cents: (-balance).max(0),
            required_cents,
        });
    }
    Ok(())
}

/// Charge `cost_cents` against the customer's credit balance.
///
/// Reads the cached balance directly (without triggering a background refresh) to
/// avoid the refresh overwriting the optimistic decrement below.  If credits are
/// sufficient the caller gets `Ok(())` immediately and the actual Stripe balance
/// transaction is created asynchronously in a spawned task with retries.
///
/// An idempotency key is attached to the Stripe POST so that retries after a
/// network error cannot produce duplicate balance transactions.
///
/// Returns `Err` only if the *cached* balance would go positive (insufficient credits).
async fn charge(
    api_key: &str,
    cost_cents: i64,
    reason: BillingReason,
    state: &StripeState,
) -> std::result::Result<(), BillingError> {
    tracing::debug!(cost_cents, "stripe::charge: starting");
    let wallet = resolve_wallet_classified(api_key, state).await?;
    // Same no-create rule as check_credit (#555): a missing customer is an
    // insufficient-credits rejection, never a trigger to create one.
    let Some(customer_id) = find_customer_by_wallet(&wallet, state)
        .await
        .map_err(BillingError::Unavailable)?
    else {
        record_billing_event(reason, &wallet, cost_cents, OUTCOME_INSUFFICIENT_CREDITS);
        return Err(BillingError::InsufficientCredits {
            available_cents: 0,
            required_cents: cost_cents,
        });
    };

    // Read the cache directly.  If missing, fall back to an inline Stripe fetch
    // using try_get_with to coalesce concurrent cache-miss requests for the same
    // customer.  We deliberately avoid `get_credit_balance()` here because its
    // background refresh could overwrite the optimistic decrement we perform below.
    let balance = match state.balance_cache.get(&customer_id).await {
        Some(cached) => cached,
        None => {
            let state2 = state.clone();
            let cid = customer_id.clone();
            state
                .balance_cache
                .try_get_with(customer_id.clone(), async move {
                    lit_billing_core::balance::fetch(&state2.client, &cid).await
                })
                .await
                .map_err(|e: Arc<anyhow::Error>| {
                    BillingError::Unavailable(anyhow::anyhow!("{e}"))
                })?
        }
    };

    if balance + cost_cents > 0 {
        record_billing_event(reason, &wallet, cost_cents, OUTCOME_INSUFFICIENT_CREDITS);
        return Err(BillingError::InsufficientCredits {
            available_cents: (-balance).max(0),
            required_cents: cost_cents,
        });
    }

    // Credits are sufficient; the charge is accepted at this point (the actual
    // Stripe balance transaction settles asynchronously below).
    record_billing_event(reason, &wallet, cost_cents, OUTCOME_ACCEPTED);

    // Optimistic local decrement: update the cached balance so subsequent calls
    // within the TTL window see the reduced value instead of the stale pre-charge
    // amount.  This bounds overcharging to concurrent requests rather than all
    // requests within the 10-minute window.
    state
        .balance_cache
        .insert(customer_id.clone(), balance + cost_cents)
        .await;

    // Fire-and-forget: spawn the actual Stripe balance transaction so the caller
    // is not blocked on the Stripe API round-trip.  Retries up to 3 times with
    // exponential backoff to handle transient Stripe failures.
    let state = state.clone();
    let cid = customer_id.clone();
    let wallet_for_metric = wallet.clone();
    let idempotency_key = uuid::Uuid::new_v4().to_string();
    tokio::spawn(async move {
        let cost_str = cost_cents.to_string();
        let delays = [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4),
        ];
        for (attempt, delay) in std::iter::once(Duration::ZERO)
            .chain(delays.iter().copied())
            .enumerate()
        {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            match state
                .client
                .post_with_idempotency(
                    &format!("customers/{cid}/balance_transactions"),
                    &[
                        ("amount", cost_str.as_str()),
                        ("currency", "usd"),
                        ("description", "API call charge"),
                    ],
                    &idempotency_key,
                )
                .await
            {
                Ok(_) => return,
                Err(e) => {
                    if attempt < delays.len() {
                        tracing::warn!(
                            "stripe: charge attempt {} failed for customer {cid}, retrying: {e}",
                            attempt + 1
                        );
                    } else {
                        // The credit was already deducted locally but the Stripe
                        // balance transaction never landed — surface this as its own
                        // signal so settlement gaps are alertable.
                        metrics::counter!(
                            "billing.charge.settlement_failed",
                            "reason" => reason.as_str(),
                            "wallet_address" => wallet_for_metric.clone(),
                        )
                        .increment(1);
                        tracing::error!(
                            "stripe: background charge failed after {} attempts for customer {cid}: {e}",
                            attempt + 1
                        );
                    }
                }
            }
        }
    });

    Ok(())
}

/// Charge $0.01 for a management API call.
pub async fn charge_management(
    api_key: &str,
    state: &StripeState,
) -> std::result::Result<(), BillingError> {
    charge(
        api_key,
        COST_MANAGEMENT_CENTS,
        BillingReason::Management,
        state,
    )
    .await
}

/// Charge for `seconds` of Lit Action execution time.
/// Returns `Ok(())` if the charge succeeds, `Err` if insufficient credits.
pub async fn charge_lit_action_time(
    api_key: &str,
    seconds: u64,
    state: &StripeState,
) -> Result<()> {
    tracing::debug!(seconds, "stripe::charge_lit_action_time: starting");
    let seconds_i64 =
        i64::try_from(seconds).map_err(|_| anyhow::anyhow!("seconds overflow: {seconds}"))?;
    let cost = COST_LIT_ACTION_PER_SECOND_CENTS
        .checked_mul(seconds_i64)
        .ok_or_else(|| anyhow::anyhow!("cost overflow for {seconds} seconds"))?;
    if cost == 0 {
        return Ok(());
    }
    charge(api_key, cost, BillingReason::LitAction, state)
        .await
        .map_err(anyhow::Error::new)
}

/// Grant the configured starter credits to a newly created customer.
///
/// No-op when `STARTER_CREDITS_CENTS` is unset or 0 (the default). The Stripe
/// POST carries a per-customer idempotency key, so a retried `new_account`
/// cannot double-grant within Stripe's idempotency window; beyond that,
/// customers are only ever created once per account wallet, and this is only
/// called from account creation.
pub async fn grant_starter_credits(customer_id: &str, state: &StripeState) -> Result<()> {
    let cents = state.starter_credits_cents;
    if cents <= 0 {
        return Ok(());
    }
    let amount = (-cents).to_string(); // negative = credit to customer
    state
        .client
        .post_with_idempotency(
            &format!("customers/{customer_id}/balance_transactions"),
            &[
                ("amount", amount.as_str()),
                ("currency", "usd"),
                ("description", "Starter credits"),
            ],
            &format!("starter-credits-{customer_id}"),
        )
        .await?;
    state.balance_cache.invalidate(customer_id).await;
    tracing::info!(customer_id, cents, "stripe: granted starter credits");
    Ok(())
}

/// Create a PaymentIntent for `amount_cents`.  Returns `(client_secret, payment_intent_id)`.
pub async fn create_payment_intent(
    wallet_address: &str,
    amount_cents: i64,
    state: &StripeState,
) -> Result<(String, String)> {
    if amount_cents < MIN_TOPUP_CENTS {
        anyhow::bail!(
            "Minimum top-up is {} ({})",
            cents_to_display(MIN_TOPUP_CENTS),
            MIN_TOPUP_CENTS
        );
    }

    let customer_id = get_customer_by_wallet(wallet_address, state).await?;
    let amount_str = amount_cents.to_string();

    let resp = state
        .client
        .post(
            "payment_intents",
            &[
                ("amount", amount_str.as_str()),
                ("currency", "usd"),
                ("customer", &customer_id),
                ("payment_method_types[]", "card"),
            ],
        )
        .await?;

    let pi_id = resp
        .body
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe PaymentIntent: missing id"))?
        .to_string();

    let client_secret = resp
        .body
        .get("client_secret")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe PaymentIntent: missing client_secret"))?
        .to_string();

    Ok((client_secret, pi_id))
}

/// Verify a PaymentIntent succeeded and credit the customer's account.
///
/// Replay protection:
/// 1. Checks `metadata.credited == "true"` on the PaymentIntent — rejects if already applied.
/// 2. Verifies the PaymentIntent's `customer` field matches the caller's Stripe customer —
///    prevents one account from claiming another account's payment.
/// 3. Marks the PaymentIntent as credited (`metadata[credited]=true`) **before** creating
///    the balance transaction, so a crash or retry after this point is safe (the second call
///    will be rejected by check 1).
pub async fn confirm_payment_and_credit(
    payment_intent_id: &str,
    wallet_address: &str,
    state: &StripeState,
) -> Result<()> {
    let resp = state
        .client
        .get(&format!("payment_intents/{payment_intent_id}"), &[])
        .await?;

    let pi_status = resp
        .body
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    if pi_status != "succeeded" {
        anyhow::bail!(
            "PaymentIntent {payment_intent_id} has status '{pi_status}', not 'succeeded'"
        );
    }

    // Replay guard: reject if this intent was already credited.
    let already_credited = resp
        .body
        .get("metadata")
        .and_then(|m| m.get("credited"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        == "true";
    if already_credited {
        anyhow::bail!("PaymentIntent {payment_intent_id} has already been credited");
    }

    // Ownership check: the PaymentIntent's customer must match the caller's customer.
    let pi_customer = resp
        .body
        .get("customer")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let customer_id = get_customer_by_wallet(wallet_address, state).await?;
    if pi_customer != customer_id {
        anyhow::bail!("PaymentIntent {payment_intent_id} does not belong to this account");
    }

    let amount = resp
        .body
        .get("amount")
        .and_then(|a| a.as_i64())
        .ok_or_else(|| anyhow::anyhow!("PaymentIntent: missing amount"))?;

    // Mark as credited before creating the balance transaction so that any subsequent
    // call with the same intent ID is rejected even if the process crashes after this point.
    state
        .client
        .post(
            &format!("payment_intents/{payment_intent_id}"),
            &[("metadata[credited]", "true")],
        )
        .await?;

    let credit = (-amount).to_string(); // negative = credit to customer
    state
        .client
        .post(
            &format!("customers/{customer_id}/balance_transactions"),
            &[
                ("amount", credit.as_str()),
                ("currency", "usd"),
                ("description", &format!("Top-up via {payment_intent_id}")),
            ],
        )
        .await?;

    // Invalidate the cached balance so the customer sees updated credits immediately.
    state.balance_cache.invalidate(&customer_id).await;

    Ok(())
}

/// Set (or update) the email on an existing Stripe customer.
pub async fn set_customer_email(customer_id: &str, email: &str, state: &StripeState) -> Result<()> {
    lit_billing_core::customer::set_email(&state.client, customer_id, email).await
}

/// Best-effort: set the customer's email in Stripe.  Never fails the caller.
pub async fn register_customer_email(wallet_address: &str, email: &str, state: &StripeState) {
    if email.trim().is_empty() {
        return;
    }
    let Ok(customer_id) = get_customer_by_wallet(wallet_address, state).await else {
        return;
    };
    let _ = set_customer_email(&customer_id, email.trim(), state).await;
}

// ─── Reporting helpers ────────────────────────────────────────────────────────
//
// Thin wrappers over `lit-billing-core::reporting` that take the cached
// `StripeState` so call sites in `bin/stripe_report.rs` don't need to know
// about the inner `StripeClient`.

/// Page over `GET /v1/customers` and return every customer, 100 at a time.
pub async fn list_all_customers(state: &StripeState) -> Result<Vec<ReportCustomer>> {
    lit_billing_core::reporting::list_all_customers(&state.client).await
}

/// Fetch all customer balance transactions created at or after `since_unix`
/// (seconds since epoch), paginating 100 at a time.
pub async fn list_balance_transactions_since(
    state: &StripeState,
    customer_id: &str,
    since_unix: i64,
) -> Result<Vec<ReportBalanceTx>> {
    lit_billing_core::reporting::list_balance_transactions_since(
        &state.client,
        customer_id,
        since_unix,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn billing_reason_labels_are_stable() {
        // These strings are emitted as metric labels (CPL-329); dashboards and
        // alerts key off them, so the mapping must not drift.
        assert_eq!(BillingReason::Management.as_str(), "management");
        assert_eq!(BillingReason::LitAction.as_str(), "lit_action");
    }

    #[test]
    fn cache_key_deterministic() {
        let k1 = cache_key("test-api-key");
        let k2 = cache_key("test-api-key");
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_different_inputs() {
        assert_ne!(cache_key("key-a"), cache_key("key-b"));
    }

    #[test]
    fn cache_key_accepts_precomputed_hash() {
        // ChainSecured callers send the wallet-derived hash directly. A raw key
        // and its keccak256 hex form must collapse to the same cache key — and
        // therefore resolve to the same on-chain account / Stripe customer.
        use crate::utils::parse_with_hash::api_key_hash;
        let raw = "test-api-key";
        let hash_hex = format!("0x{:064x}", api_key_hash(raw));
        assert_eq!(cache_key(raw), cache_key(&hash_hex));
    }

    // ── Balance cache merge logic ────────────────────────────────────────────

    #[test]
    fn balance_refresh_preserves_optimistic_decrement() {
        // charge() wrote -999, Stripe still shows -1000 (charge not landed yet).
        // Refresh should NOT overwrite.
        assert!(!should_update_balance_cache(Some(-999), -1000));
    }

    #[test]
    fn balance_refresh_updates_when_stripe_shows_less_credit() {
        // Cache says -1000, but Stripe says -900 (other charges landed).
        // Refresh should update to be conservative.
        assert!(should_update_balance_cache(Some(-1000), -900));
    }

    #[test]
    fn balance_refresh_updates_on_cache_miss() {
        // No cached value, always populate.
        assert!(should_update_balance_cache(None, -500));
    }

    #[test]
    fn balance_refresh_skips_when_equal() {
        // Same value, no need to write.
        assert!(!should_update_balance_cache(Some(-1000), -1000));
    }

    #[test]
    fn balance_refresh_preserves_multiple_decrements() {
        // Multiple charges: cache decremented to -950, Stripe still at -1000.
        assert!(!should_update_balance_cache(Some(-950), -1000));
    }

    // ── Local billing policy (CPL-330) ───────────────────────────────────────

    #[test]
    fn live_keys_are_detected() {
        assert!(is_live_key("sk_live_abc123"));
        assert!(is_live_key("rk_live_abc123"));
        assert!(is_live_key("pk_live_abc123"));
    }

    #[test]
    fn test_keys_are_not_live() {
        assert!(!is_live_key("sk_test_abc123"));
        assert!(!is_live_key("rk_test_abc123"));
        assert!(!is_live_key("pk_test_abc123"));
        // A key with "live" elsewhere must not trip the prefix check.
        assert!(!is_live_key("sk_test_iamalivekey"));
    }

    #[test]
    fn valid_test_keys_pass() {
        assert!(validate_local_test_keys("sk_test_abc", "pk_test_abc").is_ok());
        // Restricted secret keys are also acceptable.
        assert!(validate_local_test_keys("rk_test_abc", "pk_test_abc").is_ok());
    }

    #[test]
    fn live_keys_are_rejected() {
        let err = validate_local_test_keys("sk_live_abc", "pk_test_abc")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("LIVE"),
            "expected live-key message, got: {err}"
        );
        assert!(validate_local_test_keys("sk_test_abc", "pk_live_abc").is_err());
    }

    #[test]
    fn arbitrary_non_test_strings_are_rejected() {
        // The headline P2 bypass: a non-live junk string must NOT be accepted as
        // "test mode" and silently start the server.
        assert!(validate_local_test_keys("not-a-stripe-key", "also-bad").is_err());
        assert!(validate_local_test_keys("", "pk_test_abc").is_err());
    }

    #[test]
    fn secret_key_in_publishable_slot_is_rejected() {
        // The headline P1 leak: a secret in STRIPE_PUBLISHABLE_KEY would be served
        // to unauthenticated clients via /billing/stripe_config. Reject it: a
        // publishable key must start with pk_test_.
        assert!(validate_local_test_keys("sk_test_abc", "sk_test_abc").is_err());
        assert!(validate_local_test_keys("sk_test_abc", "rk_test_abc").is_err());
    }

    #[test]
    fn wrong_role_prefixes_are_rejected() {
        // A publishable key in the secret slot, and vice versa.
        assert!(validate_local_test_keys("pk_test_abc", "pk_test_abc").is_err());
    }

    // ── Startup key validation (validate_key) ────────────────────────────────

    /// Build the `anyhow::Error` shape that `client.get()` produces for a Stripe
    /// HTTP error: a `StripeError` carrying the status and body.
    fn stripe_err(status: reqwest::StatusCode, error_type: &str) -> anyhow::Error {
        anyhow::Error::new(lit_billing_core::StripeError {
            status,
            body: serde_json::json!({ "error": { "type": error_type } }),
        })
    }

    #[test]
    fn key_check_401_is_fatal_auth_failure() {
        // A revoked / wrong-environment key — the case this whole feature exists
        // to catch. Must be fatal so the server refuses to boot.
        let check = classify_key_check(&stripe_err(
            reqwest::StatusCode::UNAUTHORIZED,
            "authentication_error",
        ));
        assert!(
            matches!(check, KeyCheck::AuthFailed(_)),
            "401 must be AuthFailed, got: {check:?}"
        );
    }

    #[test]
    fn key_check_403_is_fatal_auth_failure() {
        // A restricted key lacking the required permission is also unrecoverable
        // without operator action, so it is fatal like 401.
        let check = classify_key_check(&stripe_err(
            reqwest::StatusCode::FORBIDDEN,
            "permission_error",
        ));
        assert!(
            matches!(check, KeyCheck::AuthFailed(_)),
            "403 must be AuthFailed, got: {check:?}"
        );
    }

    #[test]
    fn key_check_5xx_is_graceful_unavailable() {
        // Stripe being down says nothing about the key: degrade, don't exit.
        let check = classify_key_check(&stripe_err(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "api_error",
        ));
        assert!(
            matches!(check, KeyCheck::Unavailable(_)),
            "5xx must be Unavailable, got: {check:?}"
        );
    }

    #[test]
    fn key_check_429_rate_limit_is_graceful() {
        // A rate-limited probe is transient, not an auth verdict.
        let check = classify_key_check(&stripe_err(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "rate_limit_error",
        ));
        assert!(
            matches!(check, KeyCheck::Unavailable(_)),
            "429 must be Unavailable, got: {check:?}"
        );
    }

    #[test]
    fn key_check_transport_error_is_graceful() {
        // A timeout / connection error arrives as a plain anyhow error with no
        // StripeError on the chain — it is availability, never auth.
        let check = classify_key_check(&anyhow::anyhow!("connection timed out"));
        assert!(
            matches!(check, KeyCheck::Unavailable(_)),
            "transport error must be Unavailable, got: {check:?}"
        );
    }
}
