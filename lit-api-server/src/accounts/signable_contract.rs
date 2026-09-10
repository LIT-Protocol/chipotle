pub use crate::accounts::contracts::account_config_contract::AccountConfig;
use crate::accounts::decode_revert::decode_contract_revert;
use crate::accounts::signer_pool::SignerPool;
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
) -> Result<(AccountConfigInstance, Address, SigningClient), anyhow::Error> {
    let signer_handle = signer_pool.request().await?;
    let client = signer_handle
        .client
        .ok_or(anyhow::anyhow!("No signer available"))?;
    let signer_address = signer_handle.address;
    let contract = get_account_config_contract(client.clone()).await?;

    Ok((contract, signer_address, client))
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
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    let secret = crate::dstack::v1::get_admin_api_payer_key()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get admin api payer key: {e}"))?;
    let wallet = PrivateKeySigner::from_bytes(&B256::from_slice(&secret))?
        .with_chain_id(Some(chain_info.chain_id));

    signer_provider(wallet)
}

/// The shared read-only provider for the node's configured chain. Used by
/// callers that need a raw `eth_call` (e.g. EIP-1271 smart-contract-wallet
/// signature verification) rather than the account-config contract instance.
pub(crate) fn get_read_only_client() -> Result<SigningClient> {
    GLOBAL_READ_ONLY_CLIENT.get().cloned().ok_or_else(|| {
        anyhow::anyhow!("Read-only client not initialised — call init_chain_clients() at startup")
    })
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

/// Per-signer nonce cache, replacing alloy's `NonceFiller`.
///
/// The `NonceFiller` cache increments optimistically and is never rolled back
/// after a failed or dropped broadcast, so one RPC outage mid-send leaves the
/// signer emitting nonce-gapped transactions that can never mine (2026-09-03
/// prod incident). This cache keeps the happy path free of extra RPC calls
/// (hit → no fetch) but is **invalidated on any send failure or receipt
/// timeout**, so the next send re-derives the nonce from the chain and the
/// signer self-heals.
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

    fn get(&self, signer: Address) -> Option<u64> {
        self.lock().get(&signer).copied()
    }

    /// Record a *mined* nonce (a reverted tx still consumes its nonce). Uses
    /// max() so a late write from a slower concurrent sender (possible only
    /// via the pool's all-busy shared-lease fallback) can't regress the
    /// counter; mined nonces are chain truth, so max can never poison it.
    fn advance(&self, signer: Address, mined_nonce: u64) {
        let mut map = self.lock();
        let next = map.entry(signer).or_insert(0);
        *next = (*next).max(mined_nonce + 1);
    }

    /// Forget the signer's nonce after a failed broadcast or a receipt
    /// timeout, when we can't know whether the nonce was consumed. The next
    /// send falls back to fetching the pending nonce from the RPC.
    fn invalidate(&self, signer: Address) {
        self.lock().remove(&signer);
    }
}

static NONCE_CACHE: LazyLock<NonceCache> = LazyLock::new(NonceCache::new);

/// Fetch the signer's pending-block nonce from the RPC. Only needed on a
/// [`NONCE_CACHE`] miss: a signer's first send after startup, after a failure
/// invalidated its entry, or on the nonce-collision retry.
async fn pending_nonce(
    client: &SigningClient,
    signer_address: Address,
) -> alloy::transports::TransportResult<u64> {
    client
        .get_transaction_count(signer_address)
        .block_id(alloy::eips::BlockId::Number(BlockNumberOrTag::Pending))
        .await
}

/// Wait for a broadcast transaction to be mined, bounded by
/// [`TX_RECEIPT_TIMEOUT`], and convert the receipt into a success/failure
/// result. The bound is what keeps a transaction that never mines from
/// hanging the HTTP request forever. Updates [`NONCE_CACHE`] with the outcome:
/// mined (even reverted) advances the signer's nonce, no-receipt invalidates
/// it.
async fn wait_for_receipt(
    tx: PendingTransactionBuilder<Ethereum>,
    signer_address: Address,
    nonce: u64,
) -> Result<bool> {
    let tx_hash = *tx.tx_hash();
    match tx
        .with_timeout(Some(TX_RECEIPT_TIMEOUT))
        .get_receipt()
        .await
    {
        Ok(receipt) => {
            NONCE_CACHE.advance(signer_address, nonce);
            receipt_to_result(receipt)
        }
        Err(e) => {
            NONCE_CACHE.invalidate(signer_address);
            Err(anyhow::anyhow!(
                "no receipt for transaction {tx_hash:#x} within {TX_RECEIPT_TIMEOUT:?}: {e}"
            ))
        }
    }
}

pub async fn send_transaction<D>(
    function_call: CallBuilder<&SigningClient, D, Ethereum>,
    signer_pool: std::sync::Arc<SignerPool>,
    signer_address: Address,
    client: SigningClient,
) -> Result<bool>
where
    D: alloy::contract::CallDecoder + Clone,
{
    // Call-before-send: dry-run via eth_call so any revert surfaces as a
    // decoded, human-readable error before we broadcast. No nonce is consumed
    // and no gas is spent on a failed simulation.
    if let Err(sim_err) = function_call.call().await {
        let decoded = decode_contract_revert(&sim_err);
        if let Err(release_err) = signer_pool.release(signer_address).await {
            tracing::warn!("signer release after sim failure failed: {release_err}");
        }
        return Err(anyhow::anyhow!("Simulation failed: {decoded}"));
    }

    let nonce = match NONCE_CACHE.get(signer_address) {
        Some(nonce) => nonce,
        None => match pending_nonce(&client, signer_address).await {
            Ok(nonce) => nonce,
            Err(nonce_err) => {
                signer_pool.release(signer_address).await?;
                return Err(anyhow::anyhow!(
                    "Failed to send transaction (nonce fetch failed): {nonce_err}"
                ));
            }
        },
    };

    let first_err = match function_call.clone().nonce(nonce).send().await {
        Ok(tx) => {
            let result = wait_for_receipt(tx, signer_address, nonce).await;
            signer_pool.release(signer_address).await?;
            return result;
        }
        Err(e) => {
            // The broadcast failed after the nonce was chosen; whether it was
            // consumed is unknowable here, so make the next send re-derive it.
            NONCE_CACHE.invalidate(signer_address);
            e
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
        signer_pool.release(signer_address).await?;
        return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
    }

    // Another sender raced us to this nonce (only possible via the pool's
    // all-busy fallback, where a lease can be shared). Re-fetch and retry once.
    let fresh_nonce = match pending_nonce(&client, signer_address).await {
        Ok(nonce) => nonce,
        Err(nonce_err) => {
            tracing::warn!("nonce resync failed: {nonce_err}");
            signer_pool.release(signer_address).await?;
            return Err(anyhow::anyhow!(
                "Failed to send transaction (nonce resync failed): original error: {first_err}, nonce fetch error: {nonce_err}"
            ));
        }
    };

    let retry_call = function_call.nonce(fresh_nonce);
    let tx = match retry_call.send().await {
        Ok(tx) => tx,
        Err(retry_err) => {
            let decoded = decode_contract_revert(&retry_err);
            signer_pool.release(signer_address).await?;
            return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
        }
    };

    let result = wait_for_receipt(tx, signer_address, fresh_nonce).await;
    signer_pool.release(signer_address).await?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_cache_advances_past_mined_nonce() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);

        assert_eq!(cache.get(signer), None, "cold cache must miss");
        cache.advance(signer, 100);
        assert_eq!(cache.get(signer), Some(101));
    }

    #[test]
    fn nonce_cache_never_regresses_on_out_of_order_advance() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);

        cache.advance(signer, 101);
        // A slower concurrent sender reporting an older mined nonce last
        // (possible via the pool's shared-lease fallback) must not rewind.
        cache.advance(signer, 100);
        assert_eq!(cache.get(signer), Some(102));
    }

    #[test]
    fn nonce_cache_invalidate_forces_refetch() {
        let cache = NonceCache::new();
        let signer = Address::repeat_byte(0x11);
        let other = Address::repeat_byte(0x22);

        cache.advance(signer, 100);
        cache.advance(other, 200);
        cache.invalidate(signer);

        assert_eq!(cache.get(signer), None, "invalidated signer must miss");
        assert_eq!(cache.get(other), Some(201), "other signers unaffected");
    }
}
