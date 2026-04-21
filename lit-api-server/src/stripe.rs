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
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use moka::future::Cache;
use reqwest::StatusCode;
use tracing::instrument;

/// Cost constants in US cents.
pub const COST_MANAGEMENT_CENTS: i64 = 1; // $0.01
pub const COST_LIT_ACTION_PER_SECOND_CENTS: i64 = 1; // $0.01 per second of execution
/// Minimum top-up (500 cents = $5.00).
pub const MIN_TOPUP_CENTS: i64 = 500;

// ─── State ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct StripeState {
    // NOTE: Debug is implemented manually below to redact secret_key.
    pub publishable_key: String,
    secret_key: String,
    client: reqwest::Client,
    /// wallet_address → Stripe customer ID cache (10-min idle timeout).
    /// Avoids duplicate customer creation caused by Stripe Search API indexing lag.
    /// Uses `time_to_idle` so frequently accessed entries stay warm.
    customer_cache: Cache<String, String>,
    /// api_key → wallet_address cache.
    /// Resolves both master and usage API keys to the account's creator wallet address
    /// via the on-chain `allApiKeyHashesToMaster` mapping, avoiding a contract call per charge.
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

impl std::fmt::Debug for StripeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeState")
            .field("publishable_key", &self.publishable_key)
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

/// Initialise Stripe from environment variables.  Returns `None` if the env vars are absent
/// (billing disabled — all charges are skipped).
pub fn init() -> Option<Arc<StripeState>> {
    let secret_key = std::env::var("STRIPE_SECRET_KEY").ok()?;
    let publishable_key = std::env::var("STRIPE_PUBLISHABLE_KEY").ok()?;
    if secret_key.is_empty() || publishable_key.is_empty() {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
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
        secret_key,
        client,
        customer_cache,
        wallet_cache,
        balance_cache,
        balance_refresh_in_flight,
    }))
}

// ─── Stripe API helpers ───────────────────────────────────────────────────────

fn stripe_base() -> &'static str {
    "https://api.stripe.com/v1"
}

/// Parsed Stripe API response preserving the HTTP status code.
#[derive(Debug)]
pub struct StripeResponse {
    pub status: StatusCode,
    pub body: serde_json::Value,
}

/// Parse a Stripe API response from raw status + body text.
///
/// Accepts `(StatusCode, &str)` rather than `reqwest::Response` so this logic
/// is trivially unit-testable without mocking HTTP.
fn parse_stripe_response(status: StatusCode, body_text: &str) -> Result<StripeResponse> {
    let body: serde_json::Value = serde_json::from_str(body_text)
        .map_err(|e| anyhow::anyhow!("Stripe: invalid JSON (HTTP {status}): {e}"))?;

    if let Some(e) = body.get("error") {
        let msg = e
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("Stripe error (HTTP {status}): {msg}");
    }

    Ok(StripeResponse { status, body })
}

/// `GET /v1/<path>` with optional query params.
async fn stripe_get(
    state: &StripeState,
    path: &str,
    query: &[(&str, &str)],
) -> Result<StripeResponse> {
    let url = format!("{}/{}", stripe_base(), path);
    let resp = state
        .client
        .get(&url)
        .basic_auth(&state.secret_key, Some(""))
        .query(query)
        .send()
        .await?;
    let status = resp.status();
    let body_text = resp.text().await?;
    parse_stripe_response(status, &body_text)
}

/// `POST /v1/<path>` with form-encoded body.
async fn stripe_post(
    state: &StripeState,
    path: &str,
    params: &[(&str, &str)],
) -> Result<StripeResponse> {
    let url = format!("{}/{}", stripe_base(), path);
    let resp = state
        .client
        .post(&url)
        .basic_auth(&state.secret_key, Some(""))
        .form(params)
        .send()
        .await?;
    let status = resp.status();
    let body_text = resp.text().await?;
    parse_stripe_response(status, &body_text)
}

/// `POST /v1/<path>` with form-encoded body and an `Idempotency-Key` header.
///
/// Stripe deduplicates requests sharing the same key within 24 hours, making
/// retries after network errors safe from producing duplicate side-effects.
async fn stripe_post_with_idempotency(
    state: &StripeState,
    path: &str,
    params: &[(&str, &str)],
    idempotency_key: &str,
) -> Result<StripeResponse> {
    let url = format!("{}/{}", stripe_base(), path);
    let resp = state
        .client
        .post(&url)
        .basic_auth(&state.secret_key, Some(""))
        .header("Idempotency-Key", idempotency_key)
        .form(params)
        .send()
        .await?;
    let status = resp.status();
    let body_text = resp.text().await?;
    parse_stripe_response(status, &body_text)
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Compute a non-sensitive cache key from a raw API key string.
///
/// Uses the same keccak256 hash that the contract uses, so no secret material
/// is held in the cache's key set.  Avoids leaking raw API keys via memory
/// dumps, debug tooling, or telemetry.
fn cache_key(api_key: &str) -> String {
    crate::utils::parse_with_hash::api_key_hash(api_key).to_string()
}

/// Remove an API key from the wallet address cache.
///
/// Call this when a usage API key is deleted so that stale mappings are not served.
pub async fn invalidate_wallet_cache(api_key: &str, state: &StripeState) {
    state.wallet_cache.invalidate(&cache_key(api_key)).await;
}

/// Resolve any API key (master or usage) to the account's creator wallet address.
///
/// Uses the on-chain `allApiKeyHashesToMaster` mapping so that usage API keys
/// resolve to the same wallet (and therefore same Stripe customer) as their
/// parent account key.  Results are cached for 1 hour.
///
/// The cache is keyed by the keccak256 hash of the API key (not the raw key)
/// to avoid holding secret material in memory.
#[instrument(name = "stripe::resolve_wallet_address", skip_all, err)]
pub async fn resolve_wallet_address(api_key: &str, state: &StripeState) -> Result<String> {
    let key = cache_key(api_key);
    tracing::debug!("stripe::resolve_wallet_address: looking up wallet");
    let result = state
        .wallet_cache
        .try_get_with(key, async {
            tracing::debug!("stripe::resolve_wallet_address: cache miss, calling contract");
            crate::accounts::get_account_wallet_address(api_key).await
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
            // Search by metadata.
            let query = format!("metadata['wallet_address']:'{wallet}'");
            let resp = stripe_get(
                &state,
                "customers/search",
                &[("query", query.as_str()), ("limit", "1")],
            )
            .await?;

            if let Some(data) = resp.body.get("data").and_then(|d| d.as_array())
                && let Some(first) = data.first()
                && let Some(id) = first.get("id").and_then(|i| i.as_str())
            {
                return Ok(id.to_string());
            };

            // Not found, create a new customer
            let resp = stripe_post(
                &state,
                "customers",
                &[("metadata[wallet_address]", &wallet)],
            )
            .await?;
            let id = resp
                .body
                .get("id")
                .and_then(|i| i.as_str())
                .ok_or_else(|| anyhow::anyhow!("Stripe: missing customer id"))?;
            Ok(id.to_string())
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
                match fetch_balance(&state, &cid2).await {
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
        .try_get_with(cid, async move { fetch_balance(&state2, &cid2).await })
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

/// Fetch the raw balance from Stripe for a given customer ID.
async fn fetch_balance(state: &StripeState, customer_id: &str) -> Result<i64> {
    let resp = stripe_get(state, &format!("customers/{customer_id}"), &[]).await?;
    let balance = resp
        .body
        .get("balance")
        .and_then(|b| b.as_i64())
        .unwrap_or(0);
    tracing::debug!(customer_id, balance, "stripe::get_credit_balance: done");
    Ok(balance)
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
                    fetch_balance(&state2, &cid).await
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
            match stripe_post_with_idempotency(
                &state,
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

    let resp = stripe_post(
        state,
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
    let resp = stripe_get(state, &format!("payment_intents/{payment_intent_id}"), &[]).await?;

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
    stripe_post(
        state,
        &format!("payment_intents/{payment_intent_id}"),
        &[("metadata[credited]", "true")],
    )
    .await?;

    let credit = (-amount).to_string(); // negative = credit to customer
    stripe_post(
        state,
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
    stripe_post(
        state,
        &format!("customers/{customer_id}"),
        &[("email", email.trim())],
    )
    .await?;
    Ok(())
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

/// Format cents as a display string, e.g. 500 → "$5.00".
pub fn cents_to_display(cents: i64) -> String {
    format!("${}.{:02}", cents / 100, cents.abs() % 100)
}

// ─── Reporting helpers ────────────────────────────────────────────────────────
//
// These helpers power the `stripe_report` binary.  They are not used by the
// running API server — they are pub so that other crates in the workspace
// (and the binary) can reuse the authenticated HTTP client and the same
// customer-to-wallet mapping convention.

/// One Stripe customer as returned by `list_customers`.
#[derive(Debug, Clone)]
pub struct ReportCustomer {
    pub id: String,
    pub wallet_address: Option<String>,
    pub email: Option<String>,
}

/// One customer balance transaction as returned by `list_balance_transactions_since`.
///
/// `created` is a Unix timestamp in seconds.  `amount` is in the currency's minor unit
/// (cents for USD): positive = charge (debit to the customer's credit balance),
/// negative = credit (top-up).
#[derive(Debug, Clone)]
pub struct ReportBalanceTx {
    pub id: String,
    pub customer_id: String,
    pub amount: i64,
    pub created: i64,
    pub description: String,
}

/// Page over `GET /v1/customers` and return every customer, 100 at a time.
pub async fn list_all_customers(state: &StripeState) -> Result<Vec<ReportCustomer>> {
    let mut out = Vec::new();
    let mut starting_after: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> = vec![("limit", "100")];
        if let Some(cursor) = starting_after.as_deref() {
            query.push(("starting_after", cursor));
        }
        let resp = stripe_get(state, "customers", &query).await?;
        let data = resp
            .body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        if data.is_empty() {
            break;
        }
        for c in &data {
            let Some(id) = c.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let wallet = c
                .get("metadata")
                .and_then(|m| m.get("wallet_address"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string());
            let email = c
                .get("email")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            out.push(ReportCustomer {
                id: id.to_string(),
                wallet_address: wallet,
                email,
            });
        }
        let has_more = resp
            .body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !has_more {
            break;
        }
        // Cursor must come from the raw response's last item, not from `out`.
        // If every item in a page failed the id guard above, `out.last()` would
        // not advance and we would re-request the same page forever.
        let next_cursor = data
            .last()
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match next_cursor {
            Some(c) => starting_after = Some(c),
            None => break,
        }
    }
    Ok(out)
}

/// Fetch all customer balance transactions created at or after `since_unix`
/// (seconds since epoch), paginating 100 at a time.
pub async fn list_balance_transactions_since(
    state: &StripeState,
    customer_id: &str,
    since_unix: i64,
) -> Result<Vec<ReportBalanceTx>> {
    let path = format!("customers/{customer_id}/balance_transactions");
    let since_str = since_unix.to_string();
    let mut out = Vec::new();
    let mut starting_after: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> =
            vec![("limit", "100"), ("created[gte]", since_str.as_str())];
        if let Some(cursor) = starting_after.as_deref() {
            query.push(("starting_after", cursor));
        }
        let resp = stripe_get(state, &path, &query).await?;
        let data = resp
            .body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        if data.is_empty() {
            break;
        }
        for tx in &data {
            let Some(id) = tx.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            // Don't silently default missing amount/created to 0 — that would
            // bucket malformed transactions into 1970-01-01 with $0 and skew
            // the report. Skip with a warning instead.
            let Some(amount) = tx.get("amount").and_then(|v| v.as_i64()) else {
                tracing::warn!(
                    "stripe_report: skipping tx {id} for customer {customer_id}: missing/invalid 'amount' field"
                );
                continue;
            };
            let Some(created) = tx.get("created").and_then(|v| v.as_i64()) else {
                tracing::warn!(
                    "stripe_report: skipping tx {id} for customer {customer_id}: missing/invalid 'created' field"
                );
                continue;
            };
            let description = tx
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.push(ReportBalanceTx {
                id: id.to_string(),
                customer_id: customer_id.to_string(),
                amount,
                created,
                description,
            });
        }
        let has_more = resp
            .body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !has_more {
            break;
        }
        // See note in list_all_customers: derive the cursor from the raw response,
        // not from `out`, so a fully-filtered page can't stall the loop.
        let next_cursor = data
            .last()
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match next_cursor {
            Some(c) => starting_after = Some(c),
            None => break,
        }
    }
    Ok(out)
}

/// Convert a Unix timestamp (seconds, UTC) to a `YYYY-MM-DD` date string.
///
/// Pure function — no external deps (avoids pulling `chrono` into the server crate).
pub fn unix_to_utc_date(ts: i64) -> String {
    // Days since 1970-01-01 (Thursday), floor.
    let days = ts.div_euclid(86_400);
    // Convert to (year, month, day) using Howard Hinnant's civil_from_days algorithm.
    // https://howardhinnant.github.io/date_algorithms.html#civil_from_days
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

/// One row of the per-day-per-client usage report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    pub date: String,
    pub customer_id: String,
    pub wallet_address: Option<String>,
    pub email: Option<String>,
    /// Number of positive-amount balance transactions (charges) on this day.
    pub charges_count: u64,
    /// Sum of positive amounts in cents (debits to the customer's credit balance).
    pub charges_cents: i64,
    /// Sum of absolute value of negative amounts in cents (credits / top-ups).
    pub credits_cents: i64,
}

/// Aggregate a flat list of balance transactions into one row per
/// (date, customer_id) pair.
///
/// `customers` provides wallet/email lookup by customer id.  Transactions whose
/// `customer_id` is not present in `customers` are still bucketed but have
/// `wallet_address` and `email` set to `None`.
pub fn aggregate_report_rows(
    customers: &[ReportCustomer],
    transactions: &[ReportBalanceTx],
) -> Vec<ReportRow> {
    use std::collections::BTreeMap;
    let customer_by_id: std::collections::HashMap<&str, &ReportCustomer> =
        customers.iter().map(|c| (c.id.as_str(), c)).collect();
    // BTreeMap so output is sorted by (date, customer_id) deterministically.
    let mut buckets: BTreeMap<(String, String), ReportRow> = BTreeMap::new();
    for tx in transactions {
        let date = unix_to_utc_date(tx.created);
        let key = (date.clone(), tx.customer_id.clone());
        let row = buckets.entry(key).or_insert_with(|| {
            let cust = customer_by_id.get(tx.customer_id.as_str()).copied();
            ReportRow {
                date: date.clone(),
                customer_id: tx.customer_id.clone(),
                wallet_address: cust.and_then(|c| c.wallet_address.clone()),
                email: cust.and_then(|c| c.email.clone()),
                charges_count: 0,
                charges_cents: 0,
                credits_cents: 0,
            }
        });
        if tx.amount > 0 {
            row.charges_count += 1;
            row.charges_cents += tx.amount;
        } else if tx.amount < 0 {
            row.credits_cents += -tx.amount;
        }
    }
    buckets.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stripe_response_2xx_success() {
        let body = r#"{"id": "cus_123", "object": "customer"}"#;
        let resp = parse_stripe_response(StatusCode::OK, body).unwrap();
        assert_eq!(resp.status, StatusCode::OK);
        assert_eq!(resp.body["id"], "cus_123");
    }

    #[test]
    fn parse_stripe_response_4xx_with_error() {
        let body =
            r#"{"error": {"message": "Invalid API Key provided", "type": "authentication_error"}}"#;
        let err = parse_stripe_response(StatusCode::UNAUTHORIZED, body).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("HTTP 401"), "expected HTTP 401 in: {msg}");
        assert!(
            msg.contains("Invalid API Key provided"),
            "expected error message in: {msg}"
        );
    }

    #[test]
    fn parse_stripe_response_5xx_with_error() {
        let body = r#"{"error": {"message": "Internal server error", "type": "api_error"}}"#;
        let err = parse_stripe_response(StatusCode::INTERNAL_SERVER_ERROR, body).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("HTTP 500"), "expected HTTP 500 in: {msg}");
    }

    #[test]
    fn parse_stripe_response_error_without_message() {
        let body = r#"{"error": {"type": "api_error"}}"#;
        let err = parse_stripe_response(StatusCode::BAD_REQUEST, body).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown error"),
            "expected 'unknown error' in: {msg}"
        );
    }

    #[test]
    fn parse_stripe_response_non_json() {
        let body = "<html>Bad Gateway</html>";
        let err = parse_stripe_response(StatusCode::BAD_GATEWAY, body).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid JSON"),
            "expected 'invalid JSON' in: {msg}"
        );
        assert!(msg.contains("HTTP 502"), "expected HTTP 502 in: {msg}");
    }

    #[test]
    fn parse_stripe_response_2xx_with_no_error_field() {
        let body = r#"{"balance": -500, "currency": "usd"}"#;
        let resp = parse_stripe_response(StatusCode::OK, body).unwrap();
        assert_eq!(resp.body["balance"], -500);
    }

    #[test]
    fn cents_to_display_whole_dollars() {
        assert_eq!(cents_to_display(500), "$5.00");
        assert_eq!(cents_to_display(100), "$1.00");
        assert_eq!(cents_to_display(0), "$0.00");
    }

    #[test]
    fn cents_to_display_with_cents() {
        assert_eq!(cents_to_display(199), "$1.99");
        assert_eq!(cents_to_display(1), "$0.01");
        assert_eq!(cents_to_display(50), "$0.50");
    }

    #[test]
    fn cents_to_display_negative() {
        // NOTE: cents_to_display has a known sign-loss bug for values in -99..=-1:
        // integer division truncates toward zero, so -1/100 = 0, losing the minus sign.
        // Also, the format "$-5.00" is non-standard (convention is "-$5.00").
        // These assertions document the CURRENT behavior, not the ideal behavior.
        assert_eq!(cents_to_display(-500), "$-5.00");
        assert_eq!(cents_to_display(-1), "$0.01"); // BUG: should indicate negative
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

    // ── Reporting helpers ────────────────────────────────────────────────────

    #[test]
    fn unix_to_utc_date_epoch() {
        assert_eq!(unix_to_utc_date(0), "1970-01-01");
    }

    #[test]
    fn unix_to_utc_date_known_values() {
        // 2026-04-21 00:00:00 UTC = 1_776_729_600
        assert_eq!(unix_to_utc_date(1_776_729_600), "2026-04-21");
        // 2026-04-21 23:59:59 UTC
        assert_eq!(unix_to_utc_date(1_776_729_600 + 86_399), "2026-04-21");
        // 2026-04-22 00:00:00 UTC
        assert_eq!(unix_to_utc_date(1_776_729_600 + 86_400), "2026-04-22");
    }

    #[test]
    fn unix_to_utc_date_leap_year() {
        // 2024-02-29 is a leap day.  1709164800 = 2024-02-29 00:00:00 UTC
        assert_eq!(unix_to_utc_date(1_709_164_800), "2024-02-29");
        assert_eq!(unix_to_utc_date(1_709_251_199), "2024-02-29");
        assert_eq!(unix_to_utc_date(1_709_251_200), "2024-03-01");
    }

    fn tx(customer_id: &str, amount: i64, created: i64) -> ReportBalanceTx {
        ReportBalanceTx {
            id: format!("tx_{customer_id}_{created}_{amount}"),
            customer_id: customer_id.to_string(),
            amount,
            created,
            description: String::new(),
        }
    }

    #[test]
    fn aggregate_report_rows_empty() {
        assert!(aggregate_report_rows(&[], &[]).is_empty());
    }

    #[test]
    fn aggregate_report_rows_buckets_by_day_and_customer() {
        let customers = vec![
            ReportCustomer {
                id: "cus_a".to_string(),
                wallet_address: Some("0xA".to_string()),
                email: None,
            },
            ReportCustomer {
                id: "cus_b".to_string(),
                wallet_address: Some("0xB".to_string()),
                email: Some("b@example.com".to_string()),
            },
        ];
        let day1 = 1_776_729_600; // 2026-04-21 00:00:00 UTC
        let day2 = day1 + 86_400; // 2026-04-22 00:00:00 UTC
        let txs = vec![
            tx("cus_a", 1, day1 + 10),
            tx("cus_a", 1, day1 + 20),
            tx("cus_a", 1, day2 + 5),
            tx("cus_b", 5, day1 + 1),
            tx("cus_b", -500, day1 + 2), // top-up credit
        ];
        let rows = aggregate_report_rows(&customers, &txs);
        assert_eq!(rows.len(), 3);
        // Sorted by (date, customer_id) ascending.
        assert_eq!(rows[0].date, "2026-04-21");
        assert_eq!(rows[0].customer_id, "cus_a");
        assert_eq!(rows[0].charges_count, 2);
        assert_eq!(rows[0].charges_cents, 2);
        assert_eq!(rows[0].credits_cents, 0);
        assert_eq!(rows[0].wallet_address.as_deref(), Some("0xA"));
        assert_eq!(rows[1].date, "2026-04-21");
        assert_eq!(rows[1].customer_id, "cus_b");
        assert_eq!(rows[1].charges_count, 1);
        assert_eq!(rows[1].charges_cents, 5);
        assert_eq!(rows[1].credits_cents, 500);
        assert_eq!(rows[1].email.as_deref(), Some("b@example.com"));
        assert_eq!(rows[2].date, "2026-04-22");
        assert_eq!(rows[2].customer_id, "cus_a");
        assert_eq!(rows[2].charges_count, 1);
    }

    #[test]
    fn aggregate_report_rows_unknown_customer_still_bucketed() {
        let day1 = 1_776_729_600; // 2026-04-21 00:00:00 UTC
        let txs = vec![tx("cus_unknown", 3, day1)];
        let rows = aggregate_report_rows(&[], &txs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].customer_id, "cus_unknown");
        assert_eq!(rows[0].wallet_address, None);
        assert_eq!(rows[0].email, None);
        assert_eq!(rows[0].charges_cents, 3);
    }
}
