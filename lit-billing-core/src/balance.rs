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
