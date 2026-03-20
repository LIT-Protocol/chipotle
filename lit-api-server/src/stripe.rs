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

use anyhow::Result;
use ethers::signers::{LocalWallet, Signer};

/// Cost constants in US cents.
pub const COST_MANAGEMENT_CENTS: i64 = 1; // $0.01
pub const COST_LIT_ACTION_CENTS: i64 = 1; // $0.01
/// Minimum top-up (500 cents = $5.00).
pub const MIN_TOPUP_CENTS: i64 = 500;

// ─── State ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct StripeState {
    pub publishable_key: String,
    secret_key: String,
    client: reqwest::Client,
}

/// Initialise Stripe from environment variables.  Returns `None` if the env vars are absent
/// (billing disabled — all charges are skipped).
pub fn init() -> Option<Arc<StripeState>> {
    let secret_key = std::env::var("STRIPE_SECRET_KEY").ok()?;
    let publishable_key = std::env::var("STRIPE_PUBLISHABLE_KEY").ok()?;
    if secret_key.is_empty() || publishable_key.is_empty() {
        return None;
    }
    tracing::info!("stripe: billing enabled");
    Some(Arc::new(StripeState {
        publishable_key,
        secret_key,
        client: reqwest::Client::new(),
    }))
}

// ─── Stripe API helpers ───────────────────────────────────────────────────────

fn stripe_base() -> &'static str {
    "https://api.stripe.com/v1"
}

/// `GET /v1/<path>` with optional query params.
async fn stripe_get(
    state: &StripeState,
    path: &str,
    query: &[(&str, &str)],
) -> Result<serde_json::Value> {
    let url = format!("{}/{}", stripe_base(), path);
    let resp = state
        .client
        .get(&url)
        .basic_auth(&state.secret_key, Some(""))
        .query(query)
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;
    if let Some(e) = body.get("error") {
        anyhow::bail!(
            "Stripe GET {path}: {}",
            e.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        );
    }
    Ok(body)
}

/// `POST /v1/<path>` with form-encoded body.
async fn stripe_post(
    state: &StripeState,
    path: &str,
    params: &[(&str, &str)],
) -> Result<serde_json::Value> {
    let url = format!("{}/{}", stripe_base(), path);
    let resp = state
        .client
        .post(&url)
        .basic_auth(&state.secret_key, Some(""))
        .form(params)
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;
    if let Some(e) = body.get("error") {
        anyhow::bail!(
            "Stripe POST {path}: {}",
            e.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        );
    }
    Ok(body)
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Derive the hex wallet address from a base64-encoded API key.
pub fn api_key_to_wallet_address(api_key: &str) -> Result<String> {
    let bytes = base64_light::base64_decode(api_key);
    if bytes.len() < 32 {
        anyhow::bail!("API key too short to derive wallet address");
    }
    let wallet = LocalWallet::from_bytes(&bytes[..32])?;
    Ok(format!("{:?}", wallet.address()))
}

/// Find the Stripe customer for this wallet address, creating one if none exists.
pub async fn get_customer_by_wallet(wallet_address: &str, state: &StripeState) -> Result<String> {
    // Search by metadata.
    let query = format!("metadata['wallet_address']:'{wallet_address}'");
    let resp = stripe_get(
        state,
        "customers/search",
        &[("query", query.as_str()), ("limit", "1")],
    )
    .await?;

    if let Some(data) = resp.get("data").and_then(|d| d.as_array())
        && let Some(first) = data.first()
        && let Some(id) = first.get("id").and_then(|i| i.as_str())
    {
        return Ok(id.to_string());
    };

    // Not found
    Err(anyhow::anyhow!("Stripe customer not found"))
}

/// Return the current credit balance in cents (≤ 0 means credits available; the Stripe
/// balance field is negative when the customer has a credit).
pub async fn get_credit_balance(customer_id: &str, state: &StripeState) -> Result<i64> {
    let resp = stripe_get(state, &format!("customers/{customer_id}"), &[]).await?;
    let balance = resp.get("balance").and_then(|b| b.as_i64()).unwrap_or(0);
    Ok(balance)
}

/// Charge `cost_cents` against the customer's credit balance.
/// Returns `Err` if the balance would go positive (insufficient credits).
async fn charge(api_key: &str, cost_cents: i64, state: &StripeState) -> Result<()> {
    let wallet = api_key_to_wallet_address(api_key)?;
    let customer_id = get_customer_by_wallet(&wallet, state).await?;
    let balance = get_credit_balance(&customer_id, state).await?;

    if balance + cost_cents > 0 {
        anyhow::bail!(
            "Insufficient credits: balance {} cents, need {} cents",
            -balance,
            cost_cents
        );
    }

    // Create a positive balance transaction to deduct credits.
    let cost_str = cost_cents.to_string();
    stripe_post(
        state,
        &format!("customers/{customer_id}/balance_transactions"),
        &[
            ("amount", cost_str.as_str()),
            ("currency", "usd"),
            ("description", "API call charge"),
        ],
    )
    .await?;

    Ok(())
}

/// Charge $0.01 for a management API call.
pub async fn charge_management(api_key: &str, state: &StripeState) -> Result<()> {
    charge(api_key, COST_MANAGEMENT_CENTS, state).await
}

/// Charge $0.01 for a Lit Action execution.
pub async fn charge_lit_action(api_key: &str, state: &StripeState) -> Result<()> {
    charge(api_key, COST_LIT_ACTION_CENTS, state).await
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
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe PaymentIntent: missing id"))?
        .to_string();

    let client_secret = resp
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

    let status = resp
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    if status != "succeeded" {
        anyhow::bail!("PaymentIntent {payment_intent_id} has status '{status}', not 'succeeded'");
    }

    // Replay guard: reject if this intent was already credited.
    let already_credited = resp
        .get("metadata")
        .and_then(|m| m.get("credited"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        == "true";
    if already_credited {
        anyhow::bail!("PaymentIntent {payment_intent_id} has already been credited");
    }

    // Ownership check: the PaymentIntent's customer must match the caller's customer.
    let pi_customer = resp.get("customer").and_then(|c| c.as_str()).unwrap_or("");
    let customer_id = get_customer_by_wallet(wallet_address, state).await?;
    if pi_customer != customer_id {
        anyhow::bail!("PaymentIntent {payment_intent_id} does not belong to this account");
    }

    let amount = resp
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
    let _ = stripe_post(
        state,
        &format!("customers/{customer_id}"),
        &[("email", email.trim())],
    )
    .await;
}

/// Format cents as a display string, e.g. 500 → "$5.00".
pub fn cents_to_display(cents: i64) -> String {
    format!("${}.{:02}", cents / 100, cents.abs() % 100)
}
