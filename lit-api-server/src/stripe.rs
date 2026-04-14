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
