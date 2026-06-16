//! On-chain account-mutation event listener.
//!
//! Polls the AccountConfig contract for the permission-mutating events emitted
//! by `WritesFacet`. For each event it extracts the account `apiKeyHash` the
//! mutation touched and invalidates just that account's cached verdicts via
//! [`crate::accounts::invalidate_account_by_hash`] (which resolves the hash to
//! its wallet and bumps that account's generation — see
//! [`crate::accounts::blockchain_cache`]).
//!
//! Invalidation is per-account, so a single account's churn never flushes other
//! accounts' caches. The listener decodes only enough of each event to recover
//! its account hash; it does not enumerate usage keys (bumping the account's
//! generation invalidates the master and all usage keys at once).
//!
//! This closes the staleness window for changes made outside this process —
//! ChainSecured wallet-signed transactions sent directly to the contract, or
//! mutations performed by another replica — without waiting out the cache TTL.
//!
//! Mirrors the polling/retry structure of [`crate::restart`].

use crate::accounts::contracts::account_config_contract::AccountConfig as ac;
use crate::accounts::signable_contract::get_read_only_account_config_contract;
use crate::accounts::{clear_wallet_resolutions, invalidate_account_by_hash};
use alloy::primitives::{B256, U256};
use alloy::providers::Provider;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::HashSet;
use std::time::Duration;

/// Polling interval for checking new account-mutation events.
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Maximum number of consecutive startup failures before giving up.
const MAX_LISTENER_RETRIES: u32 = 5;

/// Expand `$mac!(EventType, |e| <account apiKeyHash>)` once per `WritesFacet`
/// permission-mutation event. Used to build both the log filter's topic0 set and
/// the decode dispatch from a single source of truth, so the two can't drift.
///
/// The closure returns the account (master) `apiKeyHash` the mutation belongs
/// to. Usage-key events carry the account hash in `accountApiKeyHash`; bumping
/// that account's generation invalidates the usage key too, so the usage hash is
/// not needed here.
macro_rules! for_each_writes_facet_event {
    ($mac:ident) => {
        $mac!(AccountCreated, |e| e.apiKeyHash);
        $mac!(AccountConvertedToChainSecured, |e| e.apiKeyHash);
        $mac!(ChainSecuredAccountOwnershipTransferred, |e| e.apiKeyHash);
        $mac!(GroupAdded, |e| e.apiKeyHash);
        $mac!(GroupUpdated, |e| e.accountApiKeyHash);
        $mac!(GroupRemoved, |e| e.apiKeyHash);
        $mac!(ActionAdded, |e| e.accountApiKeyHash);
        $mac!(ActionRemoved, |e| e.accountApiKeyHash);
        $mac!(ActionAddedToGroup, |e| e.apiKeyHash);
        $mac!(ActionRemovedFromGroup, |e| e.apiKeyHash);
        $mac!(PkpAddedToGroup, |e| e.apiKeyHash);
        $mac!(PkpRemovedFromGroup, |e| e.apiKeyHash);
        $mac!(WalletDerivationRegistered, |e| e.apiKeyHash);
        $mac!(UsageApiKeySet, |e| e.accountApiKeyHash);
        $mac!(UsageApiKeyRemoved, |e| e.accountApiKeyHash);
    };
}

/// All WritesFacet event signature hashes, used as the `eth_getLogs` topic0 set
/// so the listener only fetches account-mutation logs (not high-volume
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

/// Decode a single log against the known WritesFacet events and return the
/// account `apiKeyHash` it touched. Logs whose signature doesn't match a known
/// event (or which fail to decode) return `None`.
fn account_hash_from_log(log: &alloy::primitives::Log) -> Option<U256> {
    let topic0 = log.topics().first().copied()?;

    macro_rules! dispatch {
        ($ev:ident, $extract:expr) => {
            if topic0 == <ac::$ev as SolEvent>::SIGNATURE_HASH {
                match ac::$ev::decode_log(log) {
                    Ok(decoded) => {
                        let extract: fn(&ac::$ev) -> U256 = $extract;
                        return Some(extract(&decoded.data));
                    }
                    Err(e) => {
                        tracing::warn!(
                            event = stringify!($ev),
                            "account_events: failed to decode log: {e}"
                        );
                        return None;
                    }
                }
            }
        };
    }

    for_each_writes_facet_event!(dispatch);
    None
}

/// Invalidate the cache for a batch of logs from one poll. Account hashes are
/// deduped so each affected account is resolved + bumped at most once per poll,
/// regardless of how many of its events appear in the batch.
async fn process_logs(logs: &[alloy::rpc::types::Log]) {
    let accounts: HashSet<U256> = logs
        .iter()
        .filter_map(|log| account_hash_from_log(&log.inner))
        .collect();

    // An ownership transfer moves an account's admin wallet, which is the
    // identity we key generations by. Memoized hash→wallet resolutions for that
    // account (including its usage keys) are now stale, and we can't enumerate
    // them, so drop the whole resolution cache. Transfers are rare, so the
    // re-resolution cost is negligible. Done before the bumps below so each
    // affected account re-resolves to its current wallet.
    let ownership_transferred = logs.iter().any(|log| {
        log.inner.topics().first()
            == Some(&ac::ChainSecuredAccountOwnershipTransferred::SIGNATURE_HASH)
    });
    if ownership_transferred {
        clear_wallet_resolutions();
    }

    tracing::info!(
        log_count = logs.len(),
        account_count = accounts.len(),
        ownership_transferred,
        "Account mutation events detected — invalidating affected accounts"
    );

    for hash in accounts {
        invalidate_account_by_hash(hash).await;
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
                    process_logs(&logs).await;
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
        assert_eq!(sigs.len(), 15, "expected 15 WritesFacet event signatures");
        let unique: HashSet<_> = sigs.iter().collect();
        assert_eq!(unique.len(), sigs.len(), "signatures must be unique");
    }

    #[test]
    fn extracts_account_hash_from_apikeyhash_events() {
        let h = U256::from(0xABCDu64);
        let cases: Vec<Log> = vec![
            log_for(&ac::GroupAdded {
                apiKeyHash: h,
                groupId: U256::from(1u64),
            }),
            log_for(&ac::ActionAddedToGroup {
                apiKeyHash: h,
                groupId: U256::from(1u64),
                action: U256::from(2u64),
            }),
            log_for(&ac::PkpAddedToGroup {
                apiKeyHash: h,
                groupId: U256::from(1u64),
                pkpId: Address::ZERO,
            }),
            log_for(&ac::AccountCreated {
                apiKeyHash: h,
                admin: Address::ZERO,
                managed: true,
            }),
        ];
        for log in &cases {
            assert_eq!(account_hash_from_log(log), Some(h));
        }
    }

    #[test]
    fn extracts_account_hash_from_accountapikeyhash_events() {
        let account = U256::from(0x1111u64);
        let usage = U256::from(0x2222u64);

        // Events that name the field `accountApiKeyHash` resolve to the account,
        // not the usage key — bumping the account covers the usage key.
        let group_updated = log_for(&ac::GroupUpdated {
            accountApiKeyHash: account,
            groupId: U256::from(1u64),
        });
        assert_eq!(account_hash_from_log(&group_updated), Some(account));

        let usage_set = log_for(&ac::UsageApiKeySet {
            accountApiKeyHash: account,
            usageApiKeyHash: usage,
        });
        assert_eq!(account_hash_from_log(&usage_set), Some(account));

        let usage_removed = log_for(&ac::UsageApiKeyRemoved {
            accountApiKeyHash: account,
            usageApiKeyHash: usage,
        });
        assert_eq!(account_hash_from_log(&usage_removed), Some(account));
    }

    #[test]
    fn unknown_event_yields_none() {
        // A billing event on the same contract address must be ignored.
        let debited = log_for(&ac::ApiKeyDebited {
            apiKeyHash: U256::from(7u64),
            amount: U256::from(100u64),
        });
        assert_eq!(account_hash_from_log(&debited), None);
    }

    #[test]
    fn every_signature_decodes_to_an_account_hash() {
        // Drift guard: every filtered signature must map to an event the decode
        // dispatch handles and yield a non-None account hash.
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
                groupId: U256::from(1u64),
            }),
            log_for(&ac::GroupUpdated {
                accountApiKeyHash: account,
                groupId: U256::from(1u64),
            }),
            log_for(&ac::GroupRemoved {
                apiKeyHash: account,
                groupId: U256::from(1u64),
            }),
            log_for(&ac::ActionAdded {
                accountApiKeyHash: account,
                actionHash: U256::from(2u64),
            }),
            log_for(&ac::ActionRemoved {
                accountApiKeyHash: account,
                actionHash: U256::from(2u64),
            }),
            log_for(&ac::ActionAddedToGroup {
                apiKeyHash: account,
                groupId: U256::from(1u64),
                action: U256::from(2u64),
            }),
            log_for(&ac::ActionRemovedFromGroup {
                apiKeyHash: account,
                groupId: U256::from(1u64),
                action: U256::from(2u64),
            }),
            log_for(&ac::PkpAddedToGroup {
                apiKeyHash: account,
                groupId: U256::from(1u64),
                pkpId: Address::ZERO,
            }),
            log_for(&ac::PkpRemovedFromGroup {
                apiKeyHash: account,
                groupId: U256::from(1u64),
                pkpId: Address::ZERO,
            }),
            log_for(&ac::WalletDerivationRegistered {
                apiKeyHash: account,
                pkpId: Address::ZERO,
                derivationPath: U256::from(3u64),
            }),
            log_for(&ac::UsageApiKeySet {
                accountApiKeyHash: account,
                usageApiKeyHash: U256::from(9u64),
            }),
            log_for(&ac::UsageApiKeyRemoved {
                accountApiKeyHash: account,
                usageApiKeyHash: U256::from(9u64),
            }),
        ];
        assert_eq!(probes.len(), 15);
        for log in &probes {
            assert_eq!(account_hash_from_log(log), Some(account));
        }
    }
}
