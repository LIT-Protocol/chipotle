//! Customer-identity primitives.
//!
//! The customer-identity invariant: every Lit Stripe customer is keyed by
//! `metadata.wallet_address`. Both billing services depend on this — keep
//! this module the single source of truth.

use anyhow::Result;
use serde::Serialize;

use crate::client::StripeClient;

/// Stripe customer summary returned by lookup helpers. Balance is intentionally
/// not part of this struct — fetch it separately via [`crate::balance::fetch`]
/// for the customer(s) the caller actually wants to act on.
#[derive(Debug, Clone, Serialize)]
pub struct CustomerSummary {
    pub id: String,
    pub email: Option<String>,
    pub wallet_address: Option<String>,
}

fn parse_summary(value: &serde_json::Value) -> Option<CustomerSummary> {
    let id = value.get("id").and_then(|v| v.as_str())?.to_string();
    let email = value
        .get("email")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let wallet_address = value
        .get("metadata")
        .and_then(|m| m.get("wallet_address"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string());
    Some(CustomerSummary {
        id,
        email,
        wallet_address,
    })
}

/// Find the Stripe customer for this wallet without creating one if missing.
///
/// Returns `Ok(None)` if no customer has `metadata.wallet_address == wallet`.
pub async fn find_by_wallet(client: &StripeClient, wallet_address: &str) -> Result<Option<String>> {
    let query = format!("metadata['wallet_address']:'{wallet_address}'");
    let resp = client
        .get(
            "customers/search",
            &[("query", query.as_str()), ("limit", "1")],
        )
        .await?;
    let id = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(id)
}

/// Find the Stripe customer summary for this wallet without creating one if missing.
///
/// Returns `Ok(None)` if no customer has `metadata.wallet_address == wallet`.
pub async fn find_summary_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<Option<CustomerSummary>> {
    let query = format!("metadata['wallet_address']:'{wallet_address}'");
    let resp = client
        .get(
            "customers/search",
            &[("query", query.as_str()), ("limit", "1")],
        )
        .await?;
    let summary = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(parse_summary);
    Ok(summary)
}

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
    if let Some(id) = find_by_wallet(client, wallet_address).await? {
        return Ok(id);
    }
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

/// Search customers by email. Returns every match (Stripe allows multiple
/// customers with the same email).
///
/// Email is matched literally; callers should pass the user-provided string
/// trimmed and lowercased if they want case-insensitive behavior.
pub async fn search_by_email(client: &StripeClient, email: &str) -> Result<Vec<CustomerSummary>> {
    let query = format!("email:'{email}'");
    let resp = client
        .get(
            "customers/search",
            &[("query", query.as_str()), ("limit", "10")],
        )
        .await?;
    let data = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(data.iter().filter_map(parse_summary).collect())
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
