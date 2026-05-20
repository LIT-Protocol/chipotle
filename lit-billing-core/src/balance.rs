//! Stripe customer-balance primitives.
//!
//! Stripe represents customer credit as a *negative* balance on the customer
//! object: `balance = -500` means the customer has $5.00 of credit. Credits
//! are applied by writing balance transactions (a negative amount makes the
//! balance more negative, i.e., more credit); charges by writing positive
//! amounts.

use anyhow::Result;

use crate::client::StripeClient;

/// Fetch the raw `balance` field on the Stripe customer.
///
/// Returns the balance in cents. Negative = credit available, positive =
/// amount owed. Missing/null fields are treated as 0.
pub async fn fetch(client: &StripeClient, customer_id: &str) -> Result<i64> {
    let resp = client.get(&format!("customers/{customer_id}"), &[]).await?;
    let balance = resp
        .body
        .get("balance")
        .and_then(|b| b.as_i64())
        .unwrap_or(0);
    tracing::debug!(customer_id, balance, "stripe::fetch_balance: done");
    Ok(balance)
}

/// Write a balance transaction on a Stripe customer.
///
/// `amount_cents` is signed: positive = charge (debit credit), negative =
/// credit (top-up / grant). Currency is hard-coded to USD; if we ever take
/// other currencies this becomes a parameter.
///
/// If `idempotency_key` is `Some`, Stripe's Idempotency-Key header is set
/// so duplicate POSTs within 24 hours collapse to a single transaction.
/// Callers should pass a fresh UUID per logical attempt.
///
/// Returns the Stripe balance-transaction id (e.g. `cbtxn_…`).
pub async fn write_transaction(
    client: &StripeClient,
    customer_id: &str,
    amount_cents: i64,
    description: &str,
    idempotency_key: Option<&str>,
) -> Result<String> {
    let amount_str = amount_cents.to_string();
    let params = [
        ("amount", amount_str.as_str()),
        ("currency", "usd"),
        ("description", description),
    ];
    let path = format!("customers/{customer_id}/balance_transactions");
    let resp = match idempotency_key {
        Some(key) => client.post_with_idempotency(&path, &params, key).await?,
        None => client.post(&path, &params).await?,
    };
    let id = resp
        .body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe: missing balance_transaction id"))?
        .to_string();
    Ok(id)
}
