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

/// Search Stripe for every customer with `metadata.wallet_address == wallet`
/// and return the OLDEST match (smallest `created`).
///
/// Historic bug #555 minted duplicate customers for some wallets (a funded
/// original plus a zero-credit duplicate created by the billing guard during
/// search-index lag). Stripe search ordering is unspecified, so `limit=1`
/// could return either record non-deterministically — caching the duplicate
/// keeps a paying customer permanently rejected. Picking the oldest match is
/// deterministic and selects the original customer, which is the one funding
/// flows found and credited.
async fn search_oldest_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<Option<serde_json::Value>> {
    let query = format!("metadata['wallet_address']:'{wallet_address}'");
    let resp = client
        .get(
            "customers/search",
            &[("query", query.as_str()), ("limit", "100")],
        )
        .await?;
    let oldest = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .min_by_key(|c| c.get("created").and_then(|v| v.as_i64()).unwrap_or(i64::MAX))
        })
        .unwrap_or(None)
        .cloned();
    Ok(oldest)
}

/// Find the Stripe customer for this wallet without creating one if missing.
///
/// Returns `Ok(None)` if no customer has `metadata.wallet_address == wallet`.
/// When duplicates exist, deterministically returns the oldest (see
/// [`search_oldest_by_wallet`]).
pub async fn find_by_wallet(client: &StripeClient, wallet_address: &str) -> Result<Option<String>> {
    let id = search_oldest_by_wallet(client, wallet_address)
        .await?
        .as_ref()
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(id)
}

/// Find the Stripe customer summary for this wallet without creating one if missing.
///
/// Returns `Ok(None)` if no customer has `metadata.wallet_address == wallet`.
/// When duplicates exist, deterministically returns the oldest (see
/// [`search_oldest_by_wallet`]).
pub async fn find_summary_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<Option<CustomerSummary>> {
    let summary = search_oldest_by_wallet(client, wallet_address)
        .await?
        .as_ref()
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
///
/// The create carries a wallet-derived `Idempotency-Key`, so two callers that
/// both miss the (eventually consistent) search index and race to create —
/// e.g. two api-server replicas, or a retried account creation — get the
/// *same* customer back instead of minting duplicates. Stripe replays the
/// original response for 24h, which comfortably covers the ~1-minute window
/// in which the search index can miss a freshly created customer.
pub async fn find_or_create_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<String> {
    if let Some(id) = find_by_wallet(client, wallet_address).await? {
        return Ok(id);
    }
    let params = [("metadata[wallet_address]", wallet_address)];
    let base_key = customer_create_idempotency_key(wallet_address);
    let resp = match client
        .post_with_idempotency("customers", &params, &base_key)
        .await
    {
        Ok(resp) => resp,
        Err(first_err) => {
            // Stripe replays the FIRST response stored under an idempotency
            // key for ≥24h — including 500s. Without a fallback, one transient
            // failure would pin every create for this wallet to that replayed
            // error for a day. Re-check search (a concurrent creator may have
            // succeeded and become indexed), then retry once under a fallback
            // key. The fallback key is equally deterministic, so racers on
            // this path still converge on a single customer.
            if let Some(id) = find_by_wallet(client, wallet_address).await? {
                return Ok(id);
            }
            tracing::warn!(
                wallet_address,
                "stripe: customer create failed under primary idempotency key, \
                 retrying under fallback key: {first_err}"
            );
            client
                .post_with_idempotency("customers", &params, &format!("{base_key}-r2"))
                .await?
        }
    };
    let id = resp
        .body
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe: missing customer id"))?;
    Ok(id.to_string())
}

/// Idempotency key for creating the customer of `wallet_address`.
///
/// Deterministic per wallet so every service that creates customers through
/// this module converges on a single Stripe customer even when the search
/// index hasn't caught up yet. The wallet is used verbatim (not normalized):
/// the create params embed the same string, and Stripe rejects a reused
/// idempotency key whose params differ, so key and params must agree.
fn customer_create_idempotency_key(wallet_address: &str) -> String {
    format!("customer-create-{wallet_address}")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The create idempotency key is part of the cross-service dedup contract:
    /// every service creating a customer for the same wallet must derive the
    /// same key, or racing creators mint duplicates again. Deterministic and
    /// well under Stripe's 255-char idempotency-key limit (wallets are 42-char
    /// 0x-hex strings).
    #[test]
    fn customer_create_idempotency_key_is_deterministic_and_wallet_scoped() {
        let wallet = "0x00000000000000000000000000000000deadbeef";
        let key = customer_create_idempotency_key(wallet);
        assert_eq!(key, format!("customer-create-{wallet}"));
        assert_eq!(key, customer_create_idempotency_key(wallet));
        assert_ne!(
            key,
            customer_create_idempotency_key("0x00000000000000000000000000000000deadbee0")
        );
        assert!(key.len() <= 255);
    }
}
