//! Customer-identity primitives.
//!
//! The customer-identity invariant: every Lit Stripe customer is keyed by
//! `metadata.wallet_address`. Both billing services depend on this — keep
//! this module the single source of truth.

use anyhow::Result;

use crate::client::StripeClient;

/// Find the Stripe customer for this wallet, creating one if none exists.
///
/// Concurrency note: Stripe's Search API has indexing lag of several seconds
/// after a customer is created. Callers handling concurrent traffic for the
/// same wallet should layer their own request-coalescing cache on top of this
/// (see `lit-api-server`'s `StripeState::customer_cache`). This function
/// itself does no caching.
pub async fn find_or_create_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<String> {
    // Search by metadata.
    let query = format!("metadata['wallet_address']:'{wallet_address}'");
    let resp = client
        .get(
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
    let resp = client
        .post("customers", &[("metadata[wallet_address]", wallet_address)])
        .await?;
    let id = resp
        .body
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe: missing customer id"))?;
    Ok(id.to_string())
}

/// Set (or update) the email on an existing Stripe customer.
pub async fn set_email(client: &StripeClient, customer_id: &str, email: &str) -> Result<()> {
    client
        .post(
            &format!("customers/{customer_id}"),
            &[("email", email.trim())],
        )
        .await?;
    Ok(())
}
