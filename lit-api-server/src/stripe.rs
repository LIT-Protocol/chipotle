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

// ─── State ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct StripeState {
    pub publishable_key: String,
    client: StripeClient,
    /// wallet_address → Stripe customer ID cache (10-min idle timeout).
    /// Avoids duplicate customer creation caused by Stripe Search API indexing lag.
    /// Uses `time_to_idle` so frequently accessed entries stay warm.
    customer_cache: Cache<String, String>,
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
}

/// Initialise Stripe from environment variables.  Returns `None` if the env vars are absent
/// (billing disabled — all charges are skipped).
pub fn init() -> Option<Arc<StripeState>> {
    let secret_key = std::env::var("STRIPE_SECRET_KEY").ok()?;
    let publishable_key = std::env::var("STRIPE_PUBLISHABLE_KEY").ok()?;
    if secret_key.is_empty() || publishable_key.is_empty() {
        return None;
    }
    let client = StripeClient::new(secret_key)
        .map_err(|e| tracing::error!("stripe: failed to build HTTP client: {e}"))
        .ok()?;
    let customer_cache = Cache::builder()
        .max_capacity(10_000)
        .time_to_idle(Duration::from_secs(600)) // 10 minutes
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
    tracing::info!("stripe: billing enabled");
    Some(Arc::new(StripeState {
        publishable_key,
        client,
        customer_cache,
        wallet_cache,
        balance_cache,
        balance_refresh_in_flight,
    }))
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
        .map_err(|e: Arc<anyhow::Error>| anyhow::anyhow!("{e}"));
    tracing::debug!(
        success = result.is_ok(),
        "stripe::resolve_wallet_address: done"
    );
    result
}

/// Find the Stripe customer for this wallet address, creating one if none exists.
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
async fn charge(api_key: &str, cost_cents: i64, state: &StripeState) -> Result<()> {
    tracing::debug!(cost_cents, "stripe::charge: starting");
    let wallet = resolve_wallet_address(api_key, state).await?;
    let customer_id = get_customer_by_wallet(&wallet, state).await?;

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
                .map_err(|e: Arc<anyhow::Error>| anyhow::anyhow!("{e}"))?
        }
    };

    if balance + cost_cents > 0 {
        anyhow::bail!(
            "Insufficient credits: balance {} cents, need {} cents",
            -balance,
            cost_cents
        );
    }

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
pub async fn charge_management(api_key: &str, state: &StripeState) -> Result<()> {
    charge(api_key, COST_MANAGEMENT_CENTS, state).await
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
    charge(api_key, cost, state).await
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
}
