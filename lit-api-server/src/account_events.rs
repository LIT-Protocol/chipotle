//! On-chain account-mutation event listener.
//!
//! Polls the AccountConfig contract for the permission-mutating events emitted
//! by `WritesFacet`. When it sees *any* of them in a block range, it flushes the
//! permission cache by bumping the global generation
//! ([`blockchain_cache::invalidate_all`]).
//!
//! That's the whole job. Because invalidation is global (see
//! [`crate::accounts::blockchain_cache`]), the listener does **not** decode logs
//! or extract account/key hashes — it only needs to know *whether* a relevant
//! mutation happened. The event-signature set is used purely as the
//! `eth_getLogs` topic0 filter, so high-volume billing/config events on the same
//! contract address don't trigger pointless flushes.
//!
//! This closes the staleness window for changes made outside this process —
//! ChainSecured wallet-signed transactions sent directly to the contract, or
//! mutations performed by another replica — without waiting out the cache TTL.
//!
//! Mirrors the polling/retry structure of [`crate::restart`].

use crate::accounts::blockchain_cache;
use crate::accounts::contracts::account_config_contract::AccountConfig as ac;
use crate::accounts::signable_contract::get_read_only_account_config_contract;
use alloy::primitives::B256;
use alloy::providers::Provider;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::time::Duration;

/// Polling interval for checking new account-mutation events.
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Maximum number of consecutive startup failures before giving up.
const MAX_LISTENER_RETRIES: u32 = 5;

/// topic0 set for the `eth_getLogs` filter: every `WritesFacet` permission
/// mutation event. We only ever check whether a matching log exists, so the
/// listener never decodes these — this list exists solely to exclude unrelated
/// (e.g. billing) events emitted by the same contract address.
fn event_signatures() -> Vec<B256> {
    vec![
        ac::AccountCreated::SIGNATURE_HASH,
        ac::AccountConvertedToChainSecured::SIGNATURE_HASH,
        ac::ChainSecuredAccountOwnershipTransferred::SIGNATURE_HASH,
        ac::GroupAdded::SIGNATURE_HASH,
        ac::GroupUpdated::SIGNATURE_HASH,
        ac::GroupRemoved::SIGNATURE_HASH,
        ac::ActionAdded::SIGNATURE_HASH,
        ac::ActionRemoved::SIGNATURE_HASH,
        ac::ActionAddedToGroup::SIGNATURE_HASH,
        ac::ActionRemovedFromGroup::SIGNATURE_HASH,
        ac::PkpAddedToGroup::SIGNATURE_HASH,
        ac::PkpRemovedFromGroup::SIGNATURE_HASH,
        ac::WalletDerivationRegistered::SIGNATURE_HASH,
        ac::UsageApiKeySet::SIGNATURE_HASH,
        ac::UsageApiKeyRemoved::SIGNATURE_HASH,
    ]
}

/// Start the on-chain account-event listener as a background task.
pub fn start_account_event_listener() {
    tokio::spawn(async move {
        let mut attempt = 0u32;
        loop {
            match run_event_listener().await {
                Ok(()) => break,
                Err(e) => {
                    attempt += 1;
                    if attempt >= MAX_LISTENER_RETRIES {
                        tracing::error!(
                            attempts = attempt,
                            "Account event listener failed permanently after {MAX_LISTENER_RETRIES} attempts: {e}"
                        );
                        break;
                    }
                    let backoff = Duration::from_secs(2u64.pow(attempt.min(5)));
                    tracing::warn!(
                        attempt,
                        backoff_secs = backoff.as_secs(),
                        "Account event listener failed: {e}. Retrying..."
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    });
}

async fn run_event_listener() -> anyhow::Result<()> {
    let contract = get_read_only_account_config_contract().await?;
    let client = contract.provider();
    let address = *contract.address();
    let signatures = event_signatures();

    let start_block = client
        .get_block_number()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get initial block number: {e}"))?;

    tracing::info!(
        from_block = start_block,
        poll_interval_secs = EVENT_POLL_INTERVAL.as_secs(),
        event_count = signatures.len(),
        "Account event listener started"
    );

    let mut last_checked_block = start_block.saturating_sub(1);
    let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
    interval.tick().await;

    loop {
        interval.tick().await;

        let latest_block = match client.get_block_number().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to get latest block number: {e}");
                continue;
            }
        };

        if latest_block <= last_checked_block {
            continue;
        }

        let from_block = last_checked_block.saturating_add(1);
        let filter = Filter::new()
            .address(address)
            .event_signature(signatures.clone())
            .from_block(from_block)
            .to_block(latest_block);

        match client.get_logs(&filter).await {
            Ok(logs) => {
                if !logs.is_empty() {
                    tracing::info!(
                        log_count = logs.len(),
                        block_range = format!("{from_block}..{latest_block}"),
                        "Account mutation events detected — flushing permission cache"
                    );
                    blockchain_cache::invalidate_all();
                }
                // Only advance once logs are successfully fetched; on error we
                // retry the same range on the next tick.
                last_checked_block = latest_block;
            }
            Err(e) => {
                tracing::warn!(
                    block_range = format!("{from_block}..{latest_block}"),
                    "Failed to query account mutation events: {e}"
                );
                continue;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn event_signatures_are_complete_and_unique() {
        let sigs = event_signatures();
        // One signature per WritesFacet permission-mutation event.
        assert_eq!(sigs.len(), 15, "expected 15 WritesFacet event signatures");
        let unique: HashSet<_> = sigs.iter().collect();
        assert_eq!(unique.len(), sigs.len(), "signatures must be unique");
    }

    #[test]
    fn billing_event_is_not_in_filter() {
        // A high-volume billing event on the same contract address must not be
        // part of the topic0 filter, or every debit would flush the cache.
        let sigs = event_signatures();
        assert!(
            !sigs.contains(&ac::ApiKeyDebited::SIGNATURE_HASH),
            "billing events must be excluded from the mutation filter"
        );
    }
}
