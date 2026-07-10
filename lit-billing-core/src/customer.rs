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

/// Deterministic ordering key for choosing a wallet's canonical customer when
/// duplicates exist: most credit first, then oldest, then smallest id.
///
/// Our credit ledger stores available credit as a NEGATIVE Stripe customer
/// balance, so ascending `balance` puts the most-funded customer first — the
/// one that actually matters. `created` then `id` are stable tie-breakers so
/// two zero-credit customers minted in the same second (exactly the #555
/// duplicate race) still resolve to a single deterministic choice instead of
/// falling back to Search's unspecified order. Missing fields default so a
/// balance-less hit sorts as zero-credit and a created-less hit sorts last.
fn customer_rank_key(c: &serde_json::Value) -> (i64, i64, String) {
    let balance = c.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
    let created = c
        .get("created")
        .and_then(|v| v.as_i64())
        .unwrap_or(i64::MAX);
    let id = c
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (balance, created, id)
}

/// Search Stripe for every customer with `metadata.wallet_address == wallet`
/// and return the one to treat as the wallet's canonical customer.
///
/// Historic bug #555 minted duplicate customers for some wallets (a funded
/// original plus a zero-credit duplicate created by the billing guard during
/// search-index lag). Stripe search ordering is unspecified, so `limit=1`
/// could return either record non-deterministically — caching the duplicate
/// keeps a paying customer permanently rejected.
///
/// Selection prefers the *funded* customer and is fully deterministic (see
/// [`customer_rank_key`]). The `balance` used comes from the search hit itself,
/// so choosing the funded duplicate costs no extra Stripe calls.
async fn search_canonical_by_wallet(
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
    // A wallet with >100 Stripe customers is pathological — the #555 race makes
    // at most a handful. If we ever see it, the funded customer might be on a
    // page we didn't fetch, so surface it rather than silently pick from a
    // partial set (we still return the best of the first page).
    if resp
        .body
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        tracing::warn!(
            wallet_address,
            "stripe: >100 customers for one wallet; canonical-customer selection \
             considered only the first search page"
        );
    }
    let best = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.iter().min_by_key(|c| customer_rank_key(c)))
        .cloned();
    Ok(best)
}

/// Find the Stripe customer for this wallet without creating one if missing.
///
/// Returns `Ok(None)` if no customer has `metadata.wallet_address == wallet`.
/// When duplicates exist, deterministically returns the canonical (funded)
/// customer (see [`search_canonical_by_wallet`]).
pub async fn find_by_wallet(client: &StripeClient, wallet_address: &str) -> Result<Option<String>> {
    let id = search_canonical_by_wallet(client, wallet_address)
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
/// When duplicates exist, deterministically returns the canonical (funded)
/// customer (see [`search_canonical_by_wallet`]).
pub async fn find_summary_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<Option<CustomerSummary>> {
    let summary = search_canonical_by_wallet(client, wallet_address)
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
/// e.g. two api-server replicas, or a retried account creation — converge on
/// the *same* customer instead of minting duplicates. Stripe replays the
/// stored response (success or error) for ≥24h.
///
/// We deliberately use ONE fixed key with no second-key fallback. Retrying a
/// failed create under a *different* key would reintroduce exactly the #555
/// duplicate race: a caller that hit `idempotency_key_in_use` (the primary
/// create still in flight on another node), or one that timed out after Stripe
/// had already created the customer, would mint a second customer under the
/// fallback key. The cost of a single key is that a *stored* Stripe 5xx pins
/// creation for this wallet until Stripe prunes the key (≤24h) — far cheaper
/// than duplicating customers on every signup race. Callers retry under the
/// same key (account creation is best-effort; top-up surfaces a retryable
/// error), which replays the now-stored success once the transient clears.
pub async fn find_or_create_by_wallet(
    client: &StripeClient,
    wallet_address: &str,
) -> Result<String> {
    if let Some(id) = find_by_wallet(client, wallet_address).await? {
        return Ok(id);
    }
    let resp = client
        .post_with_idempotency(
            "customers",
            &[("metadata[wallet_address]", wallet_address)],
            &customer_create_idempotency_key(wallet_address),
        )
        .await?;
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

    /// Duplicate-healing selection (#555): among duplicate customers for one
    /// wallet, the funded one must win regardless of age, and same-second
    /// zero-credit duplicates must resolve deterministically (not by Stripe's
    /// arbitrary search order). `min_by_key(customer_rank_key)` picks the
    /// smallest key, so a smaller key means "selected".
    #[test]
    fn canonical_customer_prefers_funded_then_oldest_then_id() {
        use serde_json::json;

        // Funded (credit = negative balance) wins even though it is NEWER than
        // an older zero-credit duplicate — this is exactly Codex finding #4.
        let zero_old = json!({"id": "cus_A", "created": 100, "balance": 0});
        let funded_new = json!({"id": "cus_B", "created": 200, "balance": -5000});
        assert!(customer_rank_key(&funded_new) < customer_rank_key(&zero_old));

        // Two zero-credit duplicates created in the same second: break the tie
        // by oldest, then by smallest id — never by search order (finding #3).
        let same_sec_y = json!({"id": "cus_Y", "created": 100, "balance": 0});
        let same_sec_x = json!({"id": "cus_X", "created": 100, "balance": 0});
        assert!(customer_rank_key(&same_sec_x) < customer_rank_key(&same_sec_y));

        // Missing fields default sanely: zero credit, sorts last by age.
        let missing = json!({"id": "cus_Z"});
        assert_eq!(
            customer_rank_key(&missing),
            (0, i64::MAX, "cus_Z".to_string())
        );
    }
}
