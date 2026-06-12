//! On-chain account-mutation event listener.
//!
//! Polls the AccountConfig contract for the account/permission mutation events
//! emitted by `WritesFacet` and invalidates the corresponding entries in the
//! global blockchain permission cache ([`crate::accounts::blockchain_cache`]).
//! This reflects on-chain changes made outside this process — e.g. directly
//! against the contract, or by another API-server instance — without waiting for
//! the 60-minute cache TTL.
//!
//! Mirrors the polling/retry structure of [`crate::restart`]. Unlike the restart
//! listener (which watches a single event), it queries all WritesFacet event
//! signatures in a single `eth_getLogs` call per interval and bumps the
//! per-account cache generation for every affected `apiKeyHash`.

use crate::accounts::blockchain_cache;
use crate::accounts::contracts::account_config_contract::AccountConfig as ac;
use crate::accounts::signable_contract::get_read_only_account_config_contract;
use alloy::primitives::{B256, U256};
use alloy::providers::Provider;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::time::Duration;

/// Polling interval for checking new account-mutation events.
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Maximum number of consecutive startup failures before giving up.
const MAX_LISTENER_RETRIES: u32 = 5;

/// Expand `$mac!(EventType, |e| vec![..account hashes..])` once per `WritesFacet`
/// account/permission mutation event. Used to build both the log filter's topic
/// set and the decode dispatch from a single source of truth, so the two can
/// never drift out of sync.
///
/// The closure extracts every `apiKeyHash` whose cached permission entries are
/// affected by the event. For usage-API-key events both the account (master) key
/// and the usage key are invalidated.
macro_rules! for_each_writes_facet_event {
    ($mac:ident) => {
        $mac!(AccountCreated, |e| vec![e.apiKeyHash]);
        $mac!(AccountConvertedToChainSecured, |e| vec![e.apiKeyHash]);
        $mac!(ChainSecuredAccountOwnershipTransferred, |e| vec![
            e.apiKeyHash
        ]);
        $mac!(GroupAdded, |e| vec![e.apiKeyHash]);
        $mac!(GroupUpdated, |e| vec![e.accountApiKeyHash]);
        $mac!(GroupRemoved, |e| vec![e.apiKeyHash]);
        $mac!(ActionAdded, |e| vec![e.accountApiKeyHash]);
        $mac!(ActionRemoved, |e| vec![e.accountApiKeyHash]);
        $mac!(ActionAddedToGroup, |e| vec![e.apiKeyHash]);
        $mac!(ActionRemovedFromGroup, |e| vec![e.apiKeyHash]);
        $mac!(PkpAddedToGroup, |e| vec![e.apiKeyHash]);
        $mac!(PkpRemovedFromGroup, |e| vec![e.apiKeyHash]);
        $mac!(WalletDerivationRegistered, |e| vec![e.apiKeyHash]);
        $mac!(UsageApiKeySet, |e| vec![
            e.accountApiKeyHash,
            e.usageApiKeyHash
        ]);
        $mac!(UsageApiKeyRemoved, |e| vec![
            e.accountApiKeyHash,
            e.usageApiKeyHash
        ]);
    };
}

/// All WritesFacet event signature hashes, used as the `eth_getLogs` topic0 set
/// so the listener only fetches account-mutation logs (and not high-volume
/// billing/config events emitted by the same contract address).
fn event_signatures() -> Vec<B256> {
    let mut sigs = Vec::new();
    macro_rules! push_sig {
        ($ev:ident, $extract:expr) => {
            sigs.push(<ac::$ev as SolEvent>::SIGNATURE_HASH);
        };
    }
    for_each_writes_facet_event!(push_sig);
    sigs
}

/// Decode a single log against the known WritesFacet events and return every
/// account `apiKeyHash` whose cached entries it affects. Logs whose signature
/// doesn't match a known event (or which fail to decode) yield an empty vec.
fn account_hashes_from_log(log: &alloy::primitives::Log) -> Vec<U256> {
    let Some(topic0) = log.topics().first().copied() else {
        return Vec::new();
    };

    macro_rules! dispatch {
        ($ev:ident, $extract:expr) => {
            if topic0 == <ac::$ev as SolEvent>::SIGNATURE_HASH {
                match ac::$ev::decode_log(log) {
                    Ok(decoded) => {
                        let extract: fn(&ac::$ev) -> Vec<U256> = $extract;
                        return extract(&decoded.data);
                    }
                    Err(e) => {
                        tracing::warn!(
                            event = stringify!($ev),
                            "account_events: failed to decode log: {e}"
                        );
                        return Vec::new();
                    }
                }
            }
        };
    }

    for_each_writes_facet_event!(dispatch);
    Vec::new()
}

/// Decode a single log and invalidate the cache for every affected account hash.
fn invalidate_from_log(log: &alloy::rpc::types::Log) {
    for hash in account_hashes_from_log(&log.inner) {
        blockchain_cache::invalidate_for_hash(hash);
    }
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
                        "Account mutation events detected — invalidating cache"
                    );
                    for log in &logs {
                        invalidate_from_log(log);
                    }
                }
                // Only advance once logs are successfully processed; on error we
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
    use alloy::primitives::{Address, Log};
    use std::collections::HashSet;

    /// Build a primitives `Log` for an event, as it would arrive from the chain.
    fn log_for<E: SolEvent>(event: &E) -> Log {
        Log {
            address: Address::ZERO,
            data: event.encode_log_data(),
        }
    }

    #[test]
    fn event_signatures_are_complete_and_unique() {
        let sigs = event_signatures();
        // One signature per WritesFacet account/permission mutation event.
        assert_eq!(sigs.len(), 15, "expected 15 WritesFacet event signatures");
        let unique: HashSet<_> = sigs.iter().collect();
        assert_eq!(unique.len(), sigs.len(), "signatures must be unique");
    }

    #[test]
    fn extracts_account_hash_from_single_hash_events() {
        let h = U256::from(0xABCDu64);

        let cases: Vec<Log> = vec![
            log_for(&ac::GroupAdded {
                apiKeyHash: h,
                groupId: U256::from(1u64),
            }),
            log_for(&ac::GroupRemoved {
                apiKeyHash: h,
                groupId: U256::from(1u64),
            }),
            log_for(&ac::GroupUpdated {
                accountApiKeyHash: h,
                groupId: U256::from(1u64),
            }),
            log_for(&ac::ActionAdded {
                accountApiKeyHash: h,
                actionHash: U256::from(2u64),
            }),
            log_for(&ac::ActionRemoved {
                accountApiKeyHash: h,
                actionHash: U256::from(2u64),
            }),
            log_for(&ac::ActionAddedToGroup {
                apiKeyHash: h,
                groupId: U256::from(1u64),
                action: U256::from(2u64),
            }),
            log_for(&ac::ActionRemovedFromGroup {
                apiKeyHash: h,
                groupId: U256::from(1u64),
                action: U256::from(2u64),
            }),
            log_for(&ac::PkpAddedToGroup {
                apiKeyHash: h,
                groupId: U256::from(1u64),
                pkpId: Address::ZERO,
            }),
            log_for(&ac::PkpRemovedFromGroup {
                apiKeyHash: h,
                groupId: U256::from(1u64),
                pkpId: Address::ZERO,
            }),
            log_for(&ac::WalletDerivationRegistered {
                apiKeyHash: h,
                pkpId: Address::ZERO,
                derivationPath: U256::from(3u64),
            }),
            log_for(&ac::AccountCreated {
                apiKeyHash: h,
                admin: Address::ZERO,
                managed: true,
            }),
            log_for(&ac::AccountConvertedToChainSecured {
                apiKeyHash: h,
                newAdminWalletAddress: Address::ZERO,
            }),
            log_for(&ac::ChainSecuredAccountOwnershipTransferred {
                apiKeyHash: h,
                previousAdminWalletAddress: Address::ZERO,
                newAdminWalletAddress: Address::ZERO,
            }),
        ];

        for log in &cases {
            assert_eq!(
                account_hashes_from_log(log),
                vec![h],
                "event with topic0 {:?} should yield the single account hash",
                log.topics().first()
            );
        }
    }

    #[test]
    fn usage_key_events_invalidate_both_account_and_usage_key() {
        let account = U256::from(0x1111u64);
        let usage = U256::from(0x2222u64);

        let set = log_for(&ac::UsageApiKeySet {
            accountApiKeyHash: account,
            usageApiKeyHash: usage,
        });
        assert_eq!(account_hashes_from_log(&set), vec![account, usage]);

        let removed = log_for(&ac::UsageApiKeyRemoved {
            accountApiKeyHash: account,
            usageApiKeyHash: usage,
        });
        assert_eq!(account_hashes_from_log(&removed), vec![account, usage]);
    }

    #[test]
    fn unknown_event_yields_no_hashes() {
        // A billing event emitted by the same contract is not a WritesFacet
        // account mutation and must be ignored.
        let debited = log_for(&ac::ApiKeyDebited {
            apiKeyHash: U256::from(7u64),
            amount: U256::from(100u64),
        });
        assert!(account_hashes_from_log(&debited).is_empty());
    }

    #[test]
    fn every_signature_decodes_to_a_nonempty_result() {
        // Guards against the signature set and the decode dispatch drifting:
        // each signature we filter on must map to an event the dispatch handles.
        let account = U256::from(0x5555u64);
        let probes: Vec<Log> = vec![
            log_for(&ac::AccountCreated {
                apiKeyHash: account,
                admin: Address::ZERO,
                managed: false,
            }),
            log_for(&ac::AccountConvertedToChainSecured {
                apiKeyHash: account,
                newAdminWalletAddress: Address::ZERO,
            }),
            log_for(&ac::ChainSecuredAccountOwnershipTransferred {
                apiKeyHash: account,
                previousAdminWalletAddress: Address::ZERO,
                newAdminWalletAddress: Address::ZERO,
            }),
            log_for(&ac::GroupAdded {
                apiKeyHash: account,
                groupId: U256::ZERO,
            }),
            log_for(&ac::GroupUpdated {
                accountApiKeyHash: account,
                groupId: U256::ZERO,
            }),
            log_for(&ac::GroupRemoved {
                apiKeyHash: account,
                groupId: U256::ZERO,
            }),
            log_for(&ac::ActionAdded {
                accountApiKeyHash: account,
                actionHash: U256::ZERO,
            }),
            log_for(&ac::ActionRemoved {
                accountApiKeyHash: account,
                actionHash: U256::ZERO,
            }),
            log_for(&ac::ActionAddedToGroup {
                apiKeyHash: account,
                groupId: U256::ZERO,
                action: U256::ZERO,
            }),
            log_for(&ac::ActionRemovedFromGroup {
                apiKeyHash: account,
                groupId: U256::ZERO,
                action: U256::ZERO,
            }),
            log_for(&ac::PkpAddedToGroup {
                apiKeyHash: account,
                groupId: U256::ZERO,
                pkpId: Address::ZERO,
            }),
            log_for(&ac::PkpRemovedFromGroup {
                apiKeyHash: account,
                groupId: U256::ZERO,
                pkpId: Address::ZERO,
            }),
            log_for(&ac::WalletDerivationRegistered {
                apiKeyHash: account,
                pkpId: Address::ZERO,
                derivationPath: U256::ZERO,
            }),
            log_for(&ac::UsageApiKeySet {
                accountApiKeyHash: account,
                usageApiKeyHash: U256::from(1u64),
            }),
            log_for(&ac::UsageApiKeyRemoved {
                accountApiKeyHash: account,
                usageApiKeyHash: U256::from(1u64),
            }),
        ];

        let signatures: HashSet<_> = event_signatures().into_iter().collect();
        assert_eq!(probes.len(), signatures.len());
        for log in &probes {
            let topic0 = log.topics().first().copied().expect("event has a topic0");
            assert!(
                signatures.contains(&topic0),
                "every probe's signature must be in the filter set"
            );
            assert!(
                account_hashes_from_log(log).contains(&account),
                "dispatch must decode every filtered signature"
            );
        }
    }
}
