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

    // Lives here so the RPC fixture can install the private read-only client
    // without exposing a production setter. No live RPC or TEE is involved.
    #[tokio::test]
    async fn wallet_removal_invalidates_master_and_usage_key_caches() {
        // Run this fixture in its own test process: these OnceLocks cannot be
        // reset, and other tests intentionally exercise uninitialized clients.
        const CHILD_ENV: &str = "LIT_TEST_WALLET_REMOVAL_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "accounts::signable_contract::tests::wallet_removal_invalidates_master_and_usage_key_caches",
                    "--nocapture",
                ])
                .env(CHILD_ENV, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success() && stdout.contains("1 passed; 0 failed"),
                "isolated wallet-removal regression must run and pass: {stdout}\n{stderr}"
            );
            return;
        }

        use crate::accounts::blockchain_cache::{self, CachedPermission, GrantDeadline};
        use crate::accounts::contracts::account_config_contract::ViewsFacet::UsageApiKeyReturn;
        use alloy::primitives::{Bytes, U256};
        use alloy::rpc::client::RpcClient;
        use alloy::rpc::json_rpc::{RequestPacket, ResponsePacket};
        use alloy::sol_types::{SolCall, SolEvent};
        use alloy::transports::mock::{Asserter, MockTransport};
        use alloy::transports::{TransportError, TransportFut};
        use std::sync::{Arc, Mutex};
        // Reuse Tonic's existing tower-service re-export; no new dependency.
        use tonic::codegen::Service;

        // Asserter checks response consumption, not the outgoing RPC. Record at
        // the transport boundary so the real contract client's serialized call
        // (including its target and account/pagination arguments) is checked.
        #[derive(Clone)]
        struct RecordingTransport {
            inner: MockTransport,
            requests: Arc<Mutex<Vec<serde_json::Value>>>,
        }

        impl Service<RequestPacket> for RecordingTransport {
            type Response = ResponsePacket;
            type Error = TransportError;
            type Future = TransportFut<'static>;

            fn poll_ready(
                &mut self,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), Self::Error>> {
                self.inner.poll_ready(cx)
            }

            fn call(&mut self, request: RequestPacket) -> Self::Future {
                self.requests
                    .lock()
                    .unwrap()
                    .push(serde_json::to_value(&request).unwrap());
                self.inner.call(request)
            }
        }

        let account = U256::from(0x103001u64);
        let usage_keys = [U256::from(0x103002u64), U256::from(0x103004u64)];
        let unrelated = U256::from(0x103003u64);
        let cid = U256::from(42u64);
        let wallet = Address::repeat_byte(0x42);
        let path = U256::from(123u64);
        let contract_address = Address::repeat_byte(0x73);
        fn never<T>(result: T) -> CachedPermission<T> {
            CachedPermission::new(result, GrantDeadline::Never)
        }
        let asserter = Asserter::new();
        let response = AccountConfig::listApiKeysCall::abi_encode_returns(
            &usage_keys
                .map(|hash| UsageApiKeyReturn {
                    apiKeyHash: hash,
                    ..Default::default()
                })
                .to_vec(),
        );
        asserter.push_success(&Bytes::from(response));
        // Leave a sentinel in the queue so an accidental second RPC cannot
        // pass silently when the invalidator logs and swallows its failure.
        asserter.push_failure_msg("unexpected extra listApiKeys call");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let transport = RecordingTransport {
            inner: MockTransport::new(asserter.clone()),
            requests: requests.clone(),
        };
        let client = ProviderBuilder::new()
            .connect_client(RpcClient::new(transport, true))
            .erased();
        assert!(GLOBAL_READ_ONLY_CLIENT.set(client).is_ok());
        GLOBAL_NODE_CONFIG
            .set(crate::config::NodeConfig {
                chain: Chain::Anvil,
                contract_address: contract_address.to_string(),
            })
            .unwrap();
        blockchain_cache::init();
        let cache = blockchain_cache::get().unwrap();
        for hash in [account, usage_keys[0], usage_keys[1], unrelated] {
            cache
                .wallet_derivation_cache()
                .insert(cache.wallet_derivation_key(hash, wallet), path)
                .await;
            cache
                .use_wallet_cache()
                .insert(cache.use_wallet_key(hash, cid, wallet), never(true))
                .await;
            cache
                .execute_and_wallet_cache()
                .insert(
                    cache.execute_and_wallet_key(hash, cid, wallet),
                    never((true, true)),
                )
                .await;
            assert_eq!(
                cache
                    .wallet_derivation_cache()
                    .get(&cache.wallet_derivation_key(hash, wallet))
                    .await,
                Some(path)
            );
            assert_eq!(
                cache
                    .use_wallet_cache()
                    .get(&cache.use_wallet_key(hash, cid, wallet))
                    .await,
                Some(never(true))
            );
            assert_eq!(
                cache
                    .execute_and_wallet_cache()
                    .get(&cache.execute_and_wallet_key(hash, cid, wallet))
                    .await,
                Some(never((true, true)))
            );
        }
        let log = alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: contract_address,
                data: AccountConfig::WalletDerivationRemoved {
                    apiKeyHash: account,
                    pkpId: wallet,
                }
                .encode_log_data(),
            },
            ..Default::default()
        };
        // Duplicate logs must still expand the account only once per poll.
        crate::account_events::process_logs(&[log.clone(), log]).await;
        {
            let requests = requests.lock().unwrap();
            assert_eq!(
                requests.len(),
                1,
                "duplicate logs must issue exactly one RPC"
            );
            let request = &requests[0];
            assert_eq!(request["method"], "eth_call");
            let params = request["params"].as_array().unwrap();
            assert_eq!(params.len(), 2);
            assert_eq!(params[1], "latest");
            let transaction: alloy::rpc::types::TransactionRequest =
                serde_json::from_value(params[0].clone()).unwrap();
            assert_eq!(transaction.to, Some(contract_address.into()));
            let calldata = transaction.input.input().unwrap();
            // Verify the selector independently of the generated call binding,
            // then decode all arguments, including the current first-page limit.
            assert_eq!(
                &calldata[..4],
                &alloy::primitives::keccak256("listApiKeys(uint256,uint256,uint256)")[..4]
            );
            assert_eq!(calldata.len(), 4 + 3 * 32);
            let call = AccountConfig::listApiKeysCall::abi_decode(calldata).unwrap();
            assert_eq!(
                call.accountApiKeyHash, account,
                "must enumerate the removed wallet's account"
            );
            assert_eq!(call.pageNumber, U256::ZERO);
            assert_eq!(call.pageSize, U256::from(1000u64));
        }
        for hash in [account, usage_keys[0], usage_keys[1]] {
            assert_eq!(
                cache
                    .wallet_derivation_cache()
                    .get(&cache.wallet_derivation_key(hash, wallet))
                    .await,
                None,
                "deleted wallet derivation must miss for {hash}"
            );
            assert_eq!(
                cache
                    .use_wallet_cache()
                    .get(&cache.use_wallet_key(hash, cid, wallet))
                    .await,
                None,
                "wallet permission must miss for {hash}"
            );
            assert_eq!(
                cache
                    .execute_and_wallet_cache()
                    .get(&cache.execute_and_wallet_key(hash, cid, wallet))
                    .await,
                None,
                "combined permission must miss for {hash}"
            );
        }
        assert_eq!(
            cache
                .wallet_derivation_cache()
                .get(&cache.wallet_derivation_key(unrelated, wallet))
                .await,
            Some(path)
        );
        assert_eq!(
            cache
                .use_wallet_cache()
                .get(&cache.use_wallet_key(unrelated, cid, wallet))
                .await,
            Some(never(true))
        );
        assert_eq!(
            cache
                .execute_and_wallet_cache()
                .get(&cache.execute_and_wallet_key(unrelated, cid, wallet))
                .await,
            Some(never((true, true)))
        );
        assert_eq!(
            asserter.read_q().len(),
            1,
            "exactly one listApiKeys response must be consumed"
        );
    }

    // The deadline a grant is cached with comes from the key's on-chain
    // expiration, and sustained use cannot keep the grant past it.
    #[tokio::test]
    async fn cached_grant_is_rechecked_on_chain_at_usage_key_expiration() {
        // Own process for the same reason as the wallet-removal fixture.
        const CHILD_ENV: &str = "LIT_TEST_GRANT_EXPIRY_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "accounts::signable_contract::tests::cached_grant_is_rechecked_on_chain_at_usage_key_expiration",
                    "--nocapture",
                ])
                .env(CHILD_ENV, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success() && stdout.contains("1 passed; 0 failed"),
                "isolated grant-expiry regression must run and pass: {stdout}\n{stderr}"
            );
            return;
        }

        use crate::accounts::blockchain_cache::{self, GrantDeadline};
        use crate::accounts::can_execute_action;
        use crate::accounts::contracts::account_config_contract::ViewsFacet::UsageApiKeyReturn;
        use crate::utils::parse_with_hash::api_key_hash;
        use alloy::primitives::{Bytes, U256};
        use alloy::rpc::client::RpcClient;
        use alloy::sol_types::SolCall;
        use alloy::transports::mock::{Asserter, MockTransport};
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let unix_now = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let (expiring, never) = ("issue-165-expiring", "issue-165-never");
        let cid = U256::from(165u64);
        // On-chain expirations are whole seconds; leave at least one.
        let expiration = unix_now().as_secs() + 2;
        let deadline = Duration::from_secs(expiration);
        let usage_keys = AccountConfig::listApiKeysCall::abi_encode_returns(&vec![
            UsageApiKeyReturn {
                apiKeyHash: U256::from(1u64),
                ..Default::default()
            },
            UsageApiKeyReturn {
                apiKeyHash: api_key_hash(expiring),
                expiration: U256::from(expiration),
                ..Default::default()
            },
            UsageApiKeyReturn {
                apiKeyHash: api_key_hash(never),
                ..Default::default()
            },
        ]);
        let granted = AccountConfig::canExecuteActionCall::abi_encode_returns(&true);
        let asserter = Asserter::new();
        // Each key's first lookup: the grant, then its expiration.
        for _ in [expiring, never] {
            asserter.push_success(&Bytes::from(granted.clone()));
            asserter.push_success(&Bytes::from(usage_keys.clone()));
        }
        // The re-check once `expiring` has expired.
        asserter.push_success(&Bytes::from(
            AccountConfig::canExecuteActionCall::abi_encode_returns(&false),
        ));
        asserter.push_failure_msg("unexpected extra RPC");
        let client = ProviderBuilder::new()
            .connect_client(RpcClient::new(MockTransport::new(asserter.clone()), true))
            .erased();
        assert!(GLOBAL_READ_ONLY_CLIENT.set(client).is_ok());
        GLOBAL_NODE_CONFIG
            .set(crate::config::NodeConfig {
                chain: Chain::Anvil,
                contract_address: Address::repeat_byte(0x73).to_string(),
            })
            .unwrap();
        blockchain_cache::init();
        let cache = blockchain_cache::get().unwrap();

        // Sustained use up to the expiration is served from the cache.
        let mut uses = 0;
        while unix_now() + Duration::from_millis(150) < deadline {
            assert!(can_execute_action(expiring, cid).await.unwrap());
            assert!(can_execute_action(never, cid).await.unwrap());
            uses += 1;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(uses > 1, "{uses} uses before expiry");
        assert_eq!(
            asserter.read_q().len(),
            2,
            "only first lookups hit the chain"
        );
        let deadline_of = |api_key| async move {
            let key = cache.execute_action_key(api_key_hash(api_key), cid);
            cache
                .execute_action_cache()
                .get(&key)
                .await
                .map(|p| p.deadline)
        };
        assert_eq!(
            deadline_of(expiring).await,
            Some(GrantDeadline::At(deadline))
        );
        assert_eq!(deadline_of(never).await, Some(GrantDeadline::Never));

        tokio::time::sleep(deadline.saturating_sub(unix_now()) + Duration::from_millis(20)).await;
        assert!(
            can_execute_action(never, cid).await.unwrap(),
            "a key that never expires keeps its cached grant"
        );
        assert!(
            !can_execute_action(expiring, cid).await.unwrap(),
            "an expired key's grant must be re-checked on chain"
        );
        // The chain's denial is cached; nothing else reaches the chain.
        assert!(!can_execute_action(expiring, cid).await.unwrap());
        assert_eq!(asserter.read_q().len(), 1, "exactly one re-check");
    }

    #[tokio::test]
    async fn usage_key_set_event_rederives_grant_deadline() {
        // Own process for the same reason as the wallet-removal fixture.
        const CHILD_ENV: &str = "LIT_TEST_DEADLINE_REDERIVE_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "accounts::signable_contract::tests::usage_key_set_event_rederives_grant_deadline",
                    "--nocapture",
                ])
                .env(CHILD_ENV, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success() && stdout.contains("1 passed; 0 failed"),
                "isolated deadline re-derivation regression must run and pass: {stdout}\n{stderr}"
            );
            return;
        }

        use crate::account_events::process_logs;
        use crate::accounts::blockchain_cache::{self, GrantDeadline};
        use crate::accounts::can_execute_action;
        use crate::accounts::contracts::account_config_contract::ViewsFacet::UsageApiKeyReturn;
        use crate::utils::parse_with_hash::api_key_hash;
        use alloy::primitives::{Bytes, U256};
        use alloy::rpc::client::RpcClient;
        use alloy::sol_types::{SolCall, SolEvent};
        use alloy::transports::mock::{Asserter, MockTransport};
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let unix_now = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let key = "issue-165-expiration-added";
        let (master, hash) = (U256::from(2u64), api_key_hash(key));
        let cid = U256::from(165u64);
        let usage_keys = |expiration: u64| {
            Bytes::from(AccountConfig::listApiKeysCall::abi_encode_returns(&vec![
                UsageApiKeyReturn {
                    apiKeyHash: U256::from(1u64),
                    ..Default::default()
                },
                UsageApiKeyReturn {
                    apiKeyHash: hash,
                    expiration: U256::from(expiration),
                    ..Default::default()
                },
            ]))
        };
        let check = |allowed: bool| {
            Bytes::from(AccountConfig::canExecuteActionCall::abi_encode_returns(
                &allowed,
            ))
        };
        let asserter = Asserter::new();
        // The key starts without an expiration: the grant, then its deadline.
        asserter.push_success(&check(true));
        asserter.push_success(&usage_keys(0));
        // Its owner then sets one. On-chain expirations are whole seconds;
        // leave at least one.
        let expiration = unix_now().as_secs() + 2;
        let deadline = Duration::from_secs(expiration);
        // The listener's account expansion, then the next grant and deadline.
        // The event names the key, so the expansion does not seed it.
        asserter.push_success(&usage_keys(expiration));
        asserter.push_success(&check(true));
        asserter.push_success(&usage_keys(expiration));
        // The re-check once the new expiration has passed.
        asserter.push_success(&check(false));
        asserter.push_failure_msg("unexpected extra RPC");
        let client = ProviderBuilder::new()
            .connect_client(RpcClient::new(MockTransport::new(asserter.clone()), true))
            .erased();
        assert!(GLOBAL_READ_ONLY_CLIENT.set(client).is_ok());
        GLOBAL_NODE_CONFIG
            .set(crate::config::NodeConfig {
                chain: Chain::Anvil,
                contract_address: Address::repeat_byte(0x73).to_string(),
            })
            .unwrap();
        blockchain_cache::init();
        let cache = blockchain_cache::get().unwrap();
        let deadline_of = || async move {
            let key = cache.execute_action_key(hash, cid);
            cache
                .execute_action_cache()
                .get(&key)
                .await
                .map(|p| p.deadline)
        };

        assert!(can_execute_action(key, cid).await.unwrap());
        assert!(can_execute_action(key, cid).await.unwrap());
        assert_eq!(deadline_of().await, Some(GrantDeadline::Never));
        assert_eq!(
            asserter.read_q().len(),
            5,
            "only the first lookup hits the chain"
        );

        // `setUsageApiKey` emits `UsageApiKeySet`; the listener processes it.
        let event = AccountConfig::UsageApiKeySet {
            accountApiKeyHash: master,
            usageApiKeyHash: hash,
        };
        process_logs(&[alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: Address::ZERO,
                data: event.encode_log_data(),
            },
            ..Default::default()
        }])
        .await;
        assert!(can_execute_action(key, cid).await.unwrap());
        assert_eq!(
            deadline_of().await,
            Some(GrantDeadline::At(deadline)),
            "the event must re-read the key's expiration"
        );
        assert_eq!(asserter.read_q().len(), 2);

        // Sustained use up to the new expiration is served from the cache.
        let mut uses = 0;
        while unix_now() + Duration::from_millis(150) < deadline {
            assert!(can_execute_action(key, cid).await.unwrap());
            uses += 1;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(uses > 1, "{uses} uses before expiry");
        assert_eq!(asserter.read_q().len(), 2, "served from the cache");

        tokio::time::sleep(deadline.saturating_sub(unix_now()) + Duration::from_millis(20)).await;
        assert!(
            !can_execute_action(key, cid).await.unwrap(),
            "the new expiration must be enforced"
        );
        assert!(!can_execute_action(key, cid).await.unwrap());
        assert_eq!(asserter.read_q().len(), 1, "exactly one re-check");
    }

    /// Re-run test `name` alone in a child process with `child_env` set, as
    /// the fixtures above do: the read-only client they install is
    /// process-wide and cannot be reset. Returns whether this is the child,
    /// which runs the test body.
    fn in_isolated_child(name: &str, child_env: &str) -> bool {
        if std::env::var_os(child_env).is_some() {
            return true;
        }
        let test = format!("accounts::signable_contract::tests::{name}");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test.as_str(), "--nocapture"])
            .env(child_env, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains("1 passed; 0 failed"),
            "isolated {name} must run and pass: {stdout}\n{stderr}"
        );
        false
    }

    /// Read-only RPC fixture that answers each `eth_call` from the mock queue
    /// of its function selector, so calls in flight together cannot take each
    /// other's responses. It records the selectors called and runs `on_call`
    /// as each call is sent. With a `barrier`, no call is answered until that
    /// many are in flight.
    #[derive(Clone)]
    struct SelectorTransport {
        queues: std::sync::Arc<HashMap<[u8; 4], alloy::transports::mock::Asserter>>,
        calls: std::sync::Arc<Mutex<Vec<[u8; 4]>>>,
        on_call: std::sync::Arc<dyn Fn([u8; 4]) + Send + Sync>,
        barrier: Option<std::sync::Arc<tokio::sync::Barrier>>,
    }

    impl SelectorTransport {
        fn new(selectors: &[[u8; 4]]) -> Self {
            Self {
                queues: std::sync::Arc::new(
                    selectors
                        .iter()
                        .map(|selector| (*selector, Default::default()))
                        .collect(),
                ),
                calls: Default::default(),
                on_call: std::sync::Arc::new(|_| {}),
                barrier: None,
            }
        }

        fn push(&self, selector: [u8; 4], response: Vec<u8>) {
            self.queues[&selector].push_success(&alloy::primitives::Bytes::from(response));
        }

        /// Selectors called so far, sorted: concurrent calls have no order.
        fn calls(&self) -> Vec<[u8; 4]> {
            Self::sorted(self.calls.lock().unwrap().clone())
        }

        /// [`Self::calls`], then forget them.
        fn take_calls(&self) -> Vec<[u8; 4]> {
            Self::sorted(std::mem::take(&mut *self.calls.lock().unwrap()))
        }

        fn sorted(mut selectors: Vec<[u8; 4]>) -> Vec<[u8; 4]> {
            selectors.sort();
            selectors
        }

        /// Install as the process's read-only client and start the cache.
        fn install(&self) {
            let client = ProviderBuilder::new()
                .connect_client(alloy::rpc::client::RpcClient::new(self.clone(), true))
                .erased();
            assert!(GLOBAL_READ_ONLY_CLIENT.set(client).is_ok());
            GLOBAL_NODE_CONFIG
                .set(crate::config::NodeConfig {
                    chain: Chain::Anvil,
                    contract_address: Address::repeat_byte(0x73).to_string(),
                })
                .unwrap();
            crate::accounts::blockchain_cache::init();
        }
    }

    impl tonic::codegen::Service<alloy::rpc::json_rpc::RequestPacket> for SelectorTransport {
        type Response = alloy::rpc::json_rpc::ResponsePacket;
        type Error = alloy::transports::TransportError;
        type Future = alloy::transports::TransportFut<'static>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: alloy::rpc::json_rpc::RequestPacket) -> Self::Future {
            let json = serde_json::to_value(&request).unwrap();
            assert_eq!(json["method"], "eth_call", "unexpected RPC: {json}");
            let transaction: TransactionRequest =
                serde_json::from_value(json["params"][0].clone()).unwrap();
            let selector: [u8; 4] = transaction.input.input().unwrap()[..4].try_into().unwrap();
            self.calls.lock().unwrap().push(selector);
            (self.on_call)(selector);
            let asserter = self.queues.get(&selector).unwrap_or_else(|| {
                panic!(
                    "unexpected call to {}",
                    alloy::primitives::hex::encode(selector)
                )
            });
            let mut inner = alloy::transports::mock::MockTransport::new(asserter.clone());
            let barrier = self.barrier.clone();
            Box::pin(async move {
                if let Some(barrier) = barrier {
                    barrier.wait().await;
                }
                tonic::codegen::Service::call(&mut inner, request).await
            })
        }
    }

    #[tokio::test]
    async fn account_invalidation_seeds_usage_key_deadlines() {
        if !in_isolated_child(
            "account_invalidation_seeds_usage_key_deadlines",
            "LIT_TEST_DEADLINE_SEED_CHILD",
        ) {
            return;
        }

        use crate::account_events::process_logs;
        use crate::accounts::blockchain_cache::{self, GrantDeadline};
        use crate::accounts::can_execute_action;
        use crate::accounts::contracts::account_config_contract::ViewsFacet::UsageApiKeyReturn;
        use crate::utils::parse_with_hash::api_key_hash;
        use alloy::primitives::U256;
        use alloy::sol_types::{SolCall, SolEvent};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let list = AccountConfig::listApiKeysCall::SELECTOR;
        let check = AccountConfig::canExecuteActionCall::SELECTOR;
        let (expiring, never) = ("issue-165-seeded-expiring", "issue-165-seeded-never");
        let (master, cid) = (U256::from(0x165u64), U256::from(165u64));
        // 2100-01-01T00:00:00Z.
        let expiration = 4_102_444_800u64;
        let usage_keys = AccountConfig::listApiKeysCall::abi_encode_returns(&vec![
            UsageApiKeyReturn {
                apiKeyHash: api_key_hash(expiring),
                expiration: U256::from(expiration),
                ..Default::default()
            },
            UsageApiKeyReturn {
                apiKeyHash: api_key_hash(never),
                ..Default::default()
            },
        ]);
        let granted = AccountConfig::canExecuteActionCall::abi_encode_returns(&true);
        let disturb = Arc::new(AtomicBool::new(false));
        let rpc = SelectorTransport {
            on_call: Arc::new({
                let disturb = disturb.clone();
                move |selector| {
                    // A generation bump while the account's keys are listed,
                    // as from an API hook after its own update was mined.
                    if selector == list && disturb.swap(false, Ordering::SeqCst) {
                        blockchain_cache::invalidate_for_hash(U256::from(0xdeadu64));
                    }
                }
            }),
            ..SelectorTransport::new(&[list, check])
        };
        rpc.install();
        let cache = blockchain_cache::get().unwrap();
        let deadline_of = |api_key| async move {
            let key = cache.execute_action_key(api_key_hash(api_key), cid);
            cache
                .execute_action_cache()
                .get(&key)
                .await
                .map(|p| p.deadline)
        };
        let log = |data: alloy::primitives::LogData| alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: Address::ZERO,
                data,
            },
            ..Default::default()
        };
        let group_updated = log(AccountConfig::GroupUpdated {
            accountApiKeyHash: master,
            groupId: U256::from(1u64),
        }
        .encode_log_data());
        let sorted = SelectorTransport::sorted;
        let deadline = GrantDeadline::At(Duration::from_secs(expiration));

        // Both account-wide invalidations seed: the listener's account
        // expansion, and the API hook after a group/action/PKP change (here
        // made with the `expiring` usage key).
        for by_hook in [false, true] {
            let invalidate = || async {
                if by_hook {
                    blockchain_cache::invalidate_for_account(expiring).await;
                } else {
                    process_logs(std::slice::from_ref(&group_updated)).await;
                }
            };

            // Listing both keys seeds their deadlines; their next grants
            // then need only the permission check.
            rpc.push(list, usage_keys.clone());
            invalidate().await;
            for _ in [expiring, never] {
                rpc.push(check, granted.clone());
            }
            assert!(can_execute_action(expiring, cid).await.unwrap());
            assert!(can_execute_action(never, cid).await.unwrap());
            assert_eq!(
                rpc.take_calls(),
                sorted(vec![list, check, check]),
                "no per-key listApiKeys (by_hook: {by_hook})"
            );
            assert_eq!(deadline_of(expiring).await, Some(deadline));
            assert_eq!(deadline_of(never).await, Some(GrantDeadline::Never));

            // A bump during that read: the keys are invalidated but not
            // seeded, so the next grant reads its own expiration.
            disturb.store(true, Ordering::SeqCst);
            rpc.push(list, usage_keys.clone());
            invalidate().await;
            assert!(!disturb.load(Ordering::SeqCst), "the keys were listed");
            assert_eq!(deadline_of(expiring).await, None, "invalidated");
            rpc.push(check, granted.clone());
            rpc.push(list, usage_keys.clone());
            assert!(can_execute_action(expiring, cid).await.unwrap());
            assert_eq!(deadline_of(expiring).await, Some(deadline));
            assert_eq!(
                rpc.take_calls(),
                sorted(vec![list, check, list]),
                "by_hook: {by_hook}"
            );
        }

        // `setUsageApiKey` moved `expiring`'s expiration earlier, to
        // `expiration`, but the listener's listing is served from before the
        // change. Only the key the event did not name is seeded from it.
        let before_change = AccountConfig::listApiKeysCall::abi_encode_returns(&vec![
            UsageApiKeyReturn {
                apiKeyHash: api_key_hash(expiring),
                expiration: U256::from(expiration + 3_600),
                ..Default::default()
            },
            UsageApiKeyReturn {
                apiKeyHash: api_key_hash(never),
                ..Default::default()
            },
        ]);
        rpc.push(list, before_change);
        process_logs(std::slice::from_ref(&log(AccountConfig::UsageApiKeySet {
            accountApiKeyHash: master,
            usageApiKeyHash: api_key_hash(expiring),
        }
        .encode_log_data())))
        .await;
        for _ in [expiring, never] {
            rpc.push(check, granted.clone());
        }
        rpc.push(list, usage_keys);
        assert!(can_execute_action(expiring, cid).await.unwrap());
        assert!(can_execute_action(never, cid).await.unwrap());
        assert_eq!(rpc.take_calls(), sorted(vec![list, check, list, check]));
        assert_eq!(
            deadline_of(expiring).await,
            Some(deadline),
            "the changed key reads its own expiration"
        );
        assert_eq!(deadline_of(never).await, Some(GrantDeadline::Never));
    }

    #[tokio::test]
    async fn uncached_grant_reads_permission_and_expiration_concurrently() {
        if !in_isolated_child(
            "uncached_grant_reads_permission_and_expiration_concurrently",
            "LIT_TEST_DEADLINE_CONCURRENT_CHILD",
        ) {
            return;
        }

        use crate::accounts::blockchain_cache::{self, GrantDeadline};
        use crate::accounts::can_execute_action;
        use crate::accounts::contracts::account_config_contract::ViewsFacet::UsageApiKeyReturn;
        use crate::utils::parse_with_hash::api_key_hash;
        use alloy::primitives::U256;
        use alloy::sol_types::SolCall;
        use std::sync::Arc;

        let list = AccountConfig::listApiKeysCall::SELECTOR;
        let check = AccountConfig::canExecuteActionCall::SELECTOR;
        let (key, cid) = ("issue-165-concurrent", U256::from(165u64));
        let expiration = 4_102_444_800u64;
        // Neither read is answered until both are in flight.
        let rpc = SelectorTransport {
            barrier: Some(Arc::new(tokio::sync::Barrier::new(2))),
            ..SelectorTransport::new(&[list, check])
        };
        rpc.push(
            check,
            AccountConfig::canExecuteActionCall::abi_encode_returns(&true),
        );
        rpc.push(
            list,
            AccountConfig::listApiKeysCall::abi_encode_returns(&vec![UsageApiKeyReturn {
                apiKeyHash: api_key_hash(key),
                expiration: U256::from(expiration),
                ..Default::default()
            }]),
        );
        rpc.install();

        let granted = tokio::time::timeout(Duration::from_secs(5), can_execute_action(key, cid))
            .await
            .expect("the expiration read must not wait for the permission read");
        assert!(granted.unwrap());
        assert_eq!(rpc.calls(), SelectorTransport::sorted(vec![check, list]));
        let cache = blockchain_cache::get().unwrap();
        let cached = cache
            .execute_action_cache()
            .get(&cache.execute_action_key(api_key_hash(key), cid))
            .await
            .unwrap();
        assert_eq!(
            cached.deadline,
            GrantDeadline::At(Duration::from_secs(expiration))
        );
        assert!(can_execute_action(key, cid).await.unwrap());
        assert_eq!(rpc.calls().len(), 2, "served from the cache");
    }

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
