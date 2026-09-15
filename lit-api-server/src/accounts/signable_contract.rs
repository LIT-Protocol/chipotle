pub use crate::accounts::contracts::account_config_contract::AccountConfig;
use crate::accounts::decode_revert::decode_contract_revert;
use crate::accounts::signer_pool::{SignerLease, SignerPool};
use crate::config::GLOBAL_NODE_CONFIG;
pub use crate::utils::chain_info::Chain;
pub use alloy::contract::CallBuilder;
pub use alloy::network::{Ethereum, TransactionBuilder, TxSigner};
pub use alloy::primitives::{Address, B256};
pub use alloy::providers::{DynProvider, PendingTransactionBuilder, Provider, ProviderBuilder};
pub use alloy::rpc::types::BlockNumberOrTag;
pub use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
pub use alloy::signers::local::PrivateKeySigner;
pub use anyhow::Result;
pub use lit_core::utils::binary::hex_to_bytes;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

/// Upper bound on waiting for a broadcast transaction to be mined. The
/// configured chains mine ~2s blocks, so a healthy transaction confirms in
/// seconds; without a bound, a dropped or nonce-gapped transaction pins the
/// calling HTTP request (and its signer lease) forever. During the 2026-09-03
/// prod incident an RPC outage left two payers sending nonce-gapped
/// transactions, and every request that borrowed them hung until the client
/// gave up.
pub(crate) const TX_RECEIPT_TIMEOUT: Duration = Duration::from_secs(30);

/// Grace added to [`TX_RECEIPT_TIMEOUT`] for the *outer* deadline around
/// `get_receipt()`. Alloy's `with_timeout` only bounds the watcher's
/// `select!`; the receipt-fetch RPC awaits in `get_receipt` sit outside it
/// (see alloy heart.rs), so a blackholed RPC could outlast the watcher
/// timeout. The outer `tokio::time::timeout` is the hard bound.
pub(crate) const TX_RECEIPT_GRACE: Duration = Duration::from_secs(10);

/// Hard deadline for each single RPC round-trip in the send pipeline
/// (simulation, nonce fetch, broadcast). Alloy's HTTP transport sets no
/// request timeout of its own, so without this a blackholed RPC pins the
/// request forever even though the receipt wait is bounded.
pub(crate) const RPC_STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the receipt watcher polls for a pending transaction. Alloy
/// defaults non-local HTTP transports to 7s, but the configured chains mine
/// ~2s blocks, so 7s adds up to a full extra poll cycle of latency to every
/// write (and holds the signer lease that much longer).
pub(crate) const RPC_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The shared signing client. A single instance is held for the lifetime of
/// the process. Nonces are pinned explicitly per send from [`NONCE_CACHE`]
/// rather than left to Alloy's `NonceFiller` cache. Signer-pool leasing still
/// serializes normal use per payer, while the oldest-lease fallback remains
/// non-blocking.
pub(crate) type SigningClient = DynProvider<Ethereum>;
pub(crate) type AccountConfigInstance = AccountConfig::AccountConfigInstance<SigningClient>;

static GLOBAL_READ_ONLY_CLIENT: OnceLock<SigningClient> = OnceLock::new();

fn rpc_url() -> Result<url::Url> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    Ok(node_config.chain.rpc_url().parse()?)
}

fn read_only_provider() -> Result<SigningClient> {
    let provider = ProviderBuilder::new().connect_http(rpc_url()?).erased();
    provider.client().set_poll_interval(RPC_POLL_INTERVAL);
    Ok(provider)
}

pub(crate) fn signer_provider(wallet: PrivateKeySigner) -> Result<SigningClient> {
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url()?)
        .erased();
    provider.client().set_poll_interval(RPC_POLL_INTERVAL);
    Ok(provider)
}

/// Initialise the global read-only client. Must be called once at startup,
/// after `init_config()`, before account contract access.
pub async fn init_chain_clients() -> Result<()> {
    let provider = read_only_provider()?;
    GLOBAL_READ_ONLY_CLIENT.get_or_init(|| provider);
    Ok(())
}

pub(crate) async fn get_signable_account_config_contract(
    signer_pool: std::sync::Arc<SignerPool>,
) -> Result<(AccountConfigInstance, SignerLease, SigningClient), anyhow::Error> {
    let signer_handle = signer_pool.request().await?;
    let client = signer_handle
        .client
        .ok_or(anyhow::anyhow!("No signer available"))?;
    let lease = signer_handle.lease;
    let contract = get_account_config_contract(client.clone()).await?;

    Ok((contract, lease, client))
}

pub async fn get_account_config_contract(client: SigningClient) -> Result<AccountConfigInstance> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = Address::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}

pub async fn get_admin_api_payer_contract() -> Result<AccountConfigInstance> {
    let admin_signer = get_admin_api_signer().await?;
    let contract = get_account_config_contract(admin_signer).await?;
    Ok(contract)
}

pub async fn get_admin_api_signer() -> Result<SigningClient> {
    Ok(get_admin_api_signer_with_address().await?.0)
}

/// The admin signing client together with its own address. Callers that
/// broadcast from the admin wallet need the sender address to pin an
/// explicitly fetched nonce (all sends in this crate pin nonces; alloy's
/// `NonceFiller` stays installed but inert).
pub(crate) async fn get_admin_api_signer_with_address() -> Result<(SigningClient, Address)> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    let secret = crate::dstack::v1::get_admin_api_payer_key()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get admin api payer key: {e}"))?;
    let wallet = PrivateKeySigner::from_bytes(&B256::from_slice(&secret))?
        .with_chain_id(Some(chain_info.chain_id));
    let address = wallet.address();

    Ok((signer_provider(wallet)?, address))
}

/// The shared read-only provider for the node's configured chain. Used by
/// callers that need a raw `eth_call` (e.g. EIP-1271 smart-contract-wallet
/// signature verification) rather than the account-config contract instance.
pub(crate) fn get_read_only_client() -> Result<SigningClient> {
    GLOBAL_READ_ONLY_CLIENT.get().cloned().ok_or_else(|| {
        anyhow::anyhow!("Read-only client not initialised — call init_chain_clients() at startup")
    })
}

/// Read-only provider + the AccountConfig address, for ad-hoc scoped `sol!`
/// interfaces that target functions not yet present in the regenerated giant
/// binding (e.g. the spending-rules view from lambda-parity PR 3). Tracks the
/// workspace alloy version directly; fold callers into the generated binding
/// once it is regenerated on the canonical toolchain.
pub(crate) fn read_only_client_and_address() -> Result<(SigningClient, Address)> {
    let client = get_read_only_client()?;
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let account_config_address =
        Address::from_slice(&hex_to_bytes(&node_config.contract_address)?);
    Ok((client, account_config_address))
}

pub(crate) async fn get_read_only_account_config_contract() -> Result<AccountConfigInstance> {
    let client = GLOBAL_READ_ONLY_CLIENT
        .get()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Read-only client not initialised — call init_chain_clients() at startup"
            )
        })?
        .clone();

    let contract = get_account_config_contract(client).await?;
    Ok(contract)
}

/// Convert a mined transaction receipt into a success/failure result.
///
/// `get_receipt()` resolves successfully even when the transaction reverted
/// on-chain — the receipt simply carries an EIP-658 status of 0. Treating that
/// as success would report a failed write (e.g. a reverted `newAccount`) as
/// having succeeded, leaving callers to act on state that was never persisted.
/// We inspect the status explicitly and surface a revert as an error.
fn receipt_to_result(receipt: alloy::rpc::types::TransactionReceipt) -> Result<bool> {
    if receipt.status() {
        Ok(true)
    } else {
        Err(anyhow::anyhow!(
            "transaction reverted on-chain (tx hash: {:#x}, block: {:?})",
            receipt.transaction_hash,
            receipt.block_number
        ))
    }
}

/// Per-signer nonce allocator, replacing alloy's `NonceFiller`.
///
/// Differs from `NonceFiller` in two ways that both mattered in production:
///
/// 1. **Invalidated on failure.** `NonceFiller` increments optimistically and
///    is never rolled back after a failed or dropped broadcast, so one RPC
///    outage mid-send leaves the signer emitting nonce-gapped transactions
///    that can never mine (2026-09-03 prod incident). Here any broadcast
///    failure or receipt timeout removes the entry, and the next send
///    re-derives the nonce from the chain — the signer self-heals.
/// 2. **Reserved at read time.** `reserve`/`reserve_seeded` hand out a nonce
///    and advance the counter in the same locked operation, so two concurrent
///    borrowers of one signer (possible via the pool's all-busy fallback or a
///    force-freed stale lease) get *distinct* nonces instead of colliding on
///    the same one.
///
/// The happy path costs no RPC (reservation is a map lookup); a fetch happens
/// only on a cold or invalidated entry.
pub(crate) struct NonceCache(Mutex<HashMap<Address, u64>>);

impl NonceCache {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Address, u64>> {
        // A poisoned mutex means a panic while holding the (await-free) lock;
        // the map itself can't be left mid-update, so recover the guard.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Atomically take the signer's next nonce and advance the counter.
    /// `None` on a cold/invalidated entry — the caller must fetch the pending
    /// nonce and call [`Self::reserve_seeded`].
    fn reserve(&self, signer: Address) -> Option<u64> {
        let mut map = self.lock();
        let next = map.get_mut(&signer)?;
        let taken = *next;
        *next = taken + 1;
        Some(taken)
    }

    /// Seed from a freshly fetched pending nonce and reserve in one locked
    /// step. `max(entry, fetched)` resolves both races: a concurrent cold-start
    /// borrower who fetched the same pending nonce gets the next slot instead
    /// of a duplicate, and a stale-low entry (the chain advanced behind our
    /// back) is corrected by the fetch.
    fn reserve_seeded(&self, signer: Address, fetched: u64) -> u64 {
        let mut map = self.lock();
        let entry = map.entry(signer).or_insert(fetched);
        let taken = (*entry).max(fetched);
        *entry = taken + 1;
        taken
    }

    /// Forget the signer's counter after a failed broadcast or a receipt
    /// timeout, when we can't know whether the nonce was consumed. The next
    /// send falls back to fetching the pending nonce from the RPC. In-flight
    /// reservations already handed out are unaffected.
    fn invalidate(&self, signer: Address) {
        self.lock().remove(&signer);
    }
}

static NONCE_CACHE: LazyLock<NonceCache> = LazyLock::new(NonceCache::new);

/// Drop a signer's nonce counter so the next send re-derives it from the
/// chain. For callers outside this module (e.g. the pool's rebalancer) after
/// a failed broadcast.
pub(crate) fn invalidate_nonce(signer: Address) {
    NONCE_CACHE.invalidate(signer);
}

/// Fetch the signer's pending-block nonce from the RPC, bounded by
/// [`RPC_STEP_TIMEOUT`].
pub(crate) async fn fetch_pending_nonce(client: &SigningClient, signer: Address) -> Result<u64> {
    let fetch = client
        .get_transaction_count(signer)
        .block_id(alloy::eips::BlockId::Number(BlockNumberOrTag::Pending));
    match tokio::time::timeout(RPC_STEP_TIMEOUT, fetch).await {
        Ok(Ok(nonce)) => Ok(nonce),
        Ok(Err(e)) => Err(anyhow::anyhow!(
            "pending-nonce fetch for {signer} failed: {e}"
        )),
        Err(_) => Err(anyhow::anyhow!(
            "pending-nonce fetch for {signer} timed out after {RPC_STEP_TIMEOUT:?}"
        )),
    }
}

/// Reserve the signer's next nonce: from the cache when warm (no RPC), else
/// from a bounded pending-nonce fetch seeded into the cache.
pub(crate) async fn reserve_nonce(client: &SigningClient, signer: Address) -> Result<u64> {
    if let Some(nonce) = NONCE_CACHE.reserve(signer) {
        return Ok(nonce);
    }
    let fetched = fetch_pending_nonce(client, signer).await?;
    Ok(NONCE_CACHE.reserve_seeded(signer, fetched))
}

/// Wait for a broadcast transaction to be mined and convert the receipt into
/// a success/failure result. Two nested bounds: alloy's watcher timeout
/// (`with_timeout`) and an outer `tokio::time::timeout` — the watcher timeout
/// alone does not bound the receipt-fetch RPC awaits inside `get_receipt`
/// (they sit outside its `select!`), so a blackholed RPC could otherwise hang
/// past it. On any failure the signer's nonce counter is invalidated (the
/// nonce may or may not have been consumed); a mined receipt — even a revert,
/// which still consumes its nonce — needs no cache update because the nonce
/// was reserved up front.
pub(crate) async fn wait_for_receipt(
    tx: PendingTransactionBuilder<Ethereum>,
    signer_address: Address,
) -> Result<bool> {
    let tx_hash = *tx.tx_hash();
    let hard_bound = TX_RECEIPT_TIMEOUT + TX_RECEIPT_GRACE;
    let outcome = tokio::time::timeout(
        hard_bound,
        tx.with_timeout(Some(TX_RECEIPT_TIMEOUT)).get_receipt(),
    )
    .await;
    match outcome {
        Ok(Ok(receipt)) => receipt_to_result(receipt),
        Ok(Err(e)) => {
            NONCE_CACHE.invalidate(signer_address);
            Err(anyhow::anyhow!(
                "no receipt for transaction {tx_hash:#x} within {TX_RECEIPT_TIMEOUT:?}: {e}"
            ))
        }
        Err(_) => {
            NONCE_CACHE.invalidate(signer_address);
            Err(anyhow::anyhow!(
                "no receipt for transaction {tx_hash:#x}: receipt wait exceeded hard bound of {hard_bound:?}"
            ))
        }
    }
}

pub async fn send_transaction<D>(
    function_call: CallBuilder<&SigningClient, D, Ethereum>,
    signer_pool: std::sync::Arc<SignerPool>,
    lease: SignerLease,
    client: SigningClient,
) -> Result<bool>
where
    D: alloy::contract::CallDecoder + Clone,
{
    let signer_address = lease.address;

    // Call-before-send: dry-run via eth_call so any revert surfaces as a
    // decoded, human-readable error before we broadcast. No nonce is consumed
    // and no gas is spent on a failed simulation.
    match tokio::time::timeout(RPC_STEP_TIMEOUT, function_call.call()).await {
        Ok(Ok(_)) => {}
        Ok(Err(sim_err)) => {
            let decoded = decode_contract_revert(&sim_err);
            if let Err(release_err) = signer_pool.release(&lease).await {
                tracing::warn!("signer release after sim failure failed: {release_err}");
            }
            return Err(anyhow::anyhow!("Simulation failed: {decoded}"));
        }
        Err(_) => {
            signer_pool.release(&lease).await?;
            return Err(anyhow::anyhow!(
                "Simulation timed out after {RPC_STEP_TIMEOUT:?}"
            ));
        }
    }

    let nonce = match reserve_nonce(&client, signer_address).await {
        Ok(nonce) => nonce,
        Err(nonce_err) => {
            signer_pool.release(&lease).await?;
            return Err(anyhow::anyhow!("Failed to send transaction: {nonce_err}"));
        }
    };

    let first_err =
        match tokio::time::timeout(RPC_STEP_TIMEOUT, function_call.clone().nonce(nonce).send())
            .await
        {
            Ok(Ok(tx)) => {
                let result = wait_for_receipt(tx, signer_address).await;
                signer_pool.release(&lease).await?;
                return result;
            }
            Ok(Err(e)) => {
                // The broadcast failed after the nonce was reserved; whether it
                // was consumed is unknowable here, so make the next send
                // re-derive it.
                NONCE_CACHE.invalidate(signer_address);
                e
            }
            Err(_) => {
                NONCE_CACHE.invalidate(signer_address);
                signer_pool.release(&lease).await?;
                return Err(anyhow::anyhow!(
                    "Transaction broadcast timed out after {RPC_STEP_TIMEOUT:?}"
                ));
            }
        };

    let is_nonce_too_low = |err: &dyn std::error::Error| -> bool {
        let mut current = err;
        loop {
            let msg = current.to_string();
            if msg.contains("nonce too low")
                || msg.contains("transaction nonce is too low")
                || msg.contains("replacement transaction underpriced")
            {
                return true;
            }
            if let Some(source) = current.source() {
                current = source;
            } else {
                break;
            }
        }
        false
    };

    if !is_nonce_too_low(&first_err) {
        let decoded = decode_contract_revert(&first_err);
        signer_pool.release(&lease).await?;
        return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
    }

    // Another sender raced us to this nonce (a shared lease from the pool's
    // all-busy fallback, or a nonce consumed outside this process). Reserve a
    // fresh one and retry once; the cache was invalidated above, so this
    // re-derives from the chain unless a concurrent borrower re-seeded it.
    let fresh_nonce = match reserve_nonce(&client, signer_address).await {
        Ok(nonce) => nonce,
        Err(nonce_err) => {
            tracing::warn!("nonce resync failed: {nonce_err}");
            signer_pool.release(&lease).await?;
            return Err(anyhow::anyhow!(
                "Failed to send transaction (nonce resync failed): original error: {first_err}, nonce fetch error: {nonce_err}"
            ));
        }
    };

    let tx = match tokio::time::timeout(RPC_STEP_TIMEOUT, function_call.nonce(fresh_nonce).send())
        .await
    {
        Ok(Ok(tx)) => tx,
        Ok(Err(retry_err)) => {
            // The retry's reservation is dead too — a concurrent borrower may
            // have re-seeded the cache since the first invalidation, so this
            // second invalidation is not redundant.
            NONCE_CACHE.invalidate(signer_address);
            let decoded = decode_contract_revert(&retry_err);
            signer_pool.release(&lease).await?;
            return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
        }
        Err(_) => {
            NONCE_CACHE.invalidate(signer_address);
            signer_pool.release(&lease).await?;
            return Err(anyhow::anyhow!(
                "Transaction retry broadcast timed out after {RPC_STEP_TIMEOUT:?}"
            ));
        }
    };

    let result = wait_for_receipt(tx, signer_address).await;
    signer_pool.release(&lease).await?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_cache_cold_reserve_misses_then_seeded_reserves_advance() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);

        assert_eq!(cache.reserve(signer), None, "cold cache must miss");
        assert_eq!(cache.reserve_seeded(signer, 100), 100);
        assert_eq!(cache.reserve(signer), Some(101));
        assert_eq!(cache.reserve(signer), Some(102));
    }

    #[test]
    fn nonce_cache_concurrent_cold_starts_get_distinct_nonces() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);

        // Two borrowers both miss the cold cache and fetch the same pending
        // nonce from the RPC. Seeding must hand out distinct values.
        assert_eq!(cache.reserve_seeded(signer, 100), 100);
        assert_eq!(cache.reserve_seeded(signer, 100), 101);
    }

    #[test]
    fn nonce_cache_seed_corrects_stale_low_entry() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);

        cache.reserve_seeded(signer, 100); // counter now 101
        // The chain advanced past our counter (e.g. an external tx from the
        // same payer): a fresh fetch wins over the stale entry.
        assert_eq!(cache.reserve_seeded(signer, 105), 105);
        assert_eq!(cache.reserve(signer), Some(106));
    }

    #[test]
    fn nonce_cache_invalidate_forces_refetch_and_is_per_signer() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);
        let other = Address::repeat_byte(0x22);

        cache.reserve_seeded(signer, 100);
        cache.reserve_seeded(other, 200);
        cache.invalidate(signer);

        assert_eq!(cache.reserve(signer), None, "invalidated signer must miss");
        assert_eq!(cache.reserve(other), Some(201), "other signers unaffected");
    }
}
