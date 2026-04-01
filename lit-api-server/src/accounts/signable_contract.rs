pub use crate::accounts::contracts::account_config_contract::AccountConfig;
use crate::accounts::decode_revert::decode_contract_revert;
use crate::accounts::signer_pool::SignerPool;
use crate::config::GLOBAL_NODE_CONFIG;
pub use crate::utils::chain_info::Chain;
pub use anyhow::Result;
use ethers::contract::builders::ContractCall;
use ethers::middleware::NonceManagerMiddleware;
pub use ethers::middleware::SignerMiddleware;
pub use ethers::providers::Http;
pub use ethers::providers::Middleware;
pub use ethers::providers::Provider;
pub use ethers::signers::LocalWallet;
use ethers::signers::Signer;
pub use ethers::types::BlockId;
pub use ethers::types::BlockNumber;
pub use ethers::types::H160;
pub use lit_core::utils::binary::hex_to_bytes;
pub use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

/// The shared signing client. A single instance is held for the lifetime of
/// the process so that `NonceManagerMiddleware` can manage nonces atomically
/// across concurrent requests. Without this, every request would create a
/// fresh `SignerMiddleware`, each fetching the same pending nonce from the
/// chain and causing "nonce too low" / "replacement transaction underpriced"
/// errors under concurrency.
pub(crate) type SigningClient =
    NonceManagerMiddleware<SignerMiddleware<Provider<Http>, LocalWallet>>;

static GLOBAL_READ_ONLY_CLIENT: OnceLock<Arc<Provider<Http>>> = OnceLock::new();

/// Initialise the global signing client. Must be called once at startup,
/// after `init_config()`, before any transactions are sent.
pub async fn init_chain_clients() -> Result<()> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let rpc_url = node_config.chain.rpc_url();

    let provider = Provider::<Http>::try_from(rpc_url.as_str())?.interval(Duration::from_secs(2));
    GLOBAL_READ_ONLY_CLIENT.get_or_init(|| Arc::new(provider));
    Ok(())
}

pub(crate) async fn get_signable_account_config_contract(
    signer_pool: Arc<SignerPool>,
) -> Result<(AccountConfig<SigningClient>, H160, Arc<SigningClient>), anyhow::Error> {
    let signer_handle = signer_pool.request().await?;
    let client = signer_handle
        .client
        .ok_or(anyhow::anyhow!("No signer available"))?;
    let signer_address = signer_handle.address;
    let contract = get_account_config_contract::<SigningClient>(client.clone()).await?;

    Ok((contract, signer_address, client))
}

pub async fn get_account_config_contract<M>(client: Arc<M>) -> Result<AccountConfig<M>>
where
    M: ethers::providers::Middleware,
{
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = H160::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}

pub async fn get_admin_api_payer_contract() -> Result<AccountConfig<SigningClient>> {
    let admin_signer = get_admin_api_signer().await?;
    let contract = get_account_config_contract::<SigningClient>(Arc::new(admin_signer)).await?;
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
    let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);
    let address = wallet.address();
    let rpc_url = node_config.chain.rpc_url();
    let provider = Provider::<Http>::try_from(rpc_url.as_str())?.interval(Duration::from_secs(2));
    let signer = SignerMiddleware::new(provider, wallet);
    let nonce_manager = NonceManagerMiddleware::new(signer, address);

    Ok(nonce_manager)
}

pub(crate) async fn get_read_only_account_config_contract()
-> Result<AccountConfig<Provider<Http>>, anyhow::Error> {
    let client = GLOBAL_READ_ONLY_CLIENT
        .get()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Read-only client not initialised — call init_chain_clients() at startup"
            )
        })?
        .clone();

    let contract = get_account_config_contract::<Provider<Http>>(client).await?;
    Ok(contract)
}

pub async fn send_transaction<T: ethers::abi::Detokenize>(
    mut function_call: ContractCall<SigningClient, T>,
    signer_pool: Arc<SignerPool>,
    signer_address: H160,
    client: Arc<SigningClient>,
) -> Result<bool> {
    // First attempt.  The Ok arm returns early so the PendingTransaction's borrow of
    // `function_call` is fully consumed before we ever reach the nonce-resync path below;
    // this lets the borrow checker accept the later mutable borrow of `function_call.tx`.
    let first_err = match function_call.send().await {
        Ok(tx) => {
            let result = match tx.await {
                Ok(_) => Ok(true),
                Err(e) => Err(anyhow::Error::from(e)),
            };
            signer_pool.release(signer_address).await?;
            return result;
        }
        Err(e) => e,
    };

    // Only nonce-too-low is worth retrying; anything else is a hard failure.
    // Walk the error chain and look for known nonce-related messages in a centralized way.
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

    // Fetch the current pending nonce from the chain and pin it on the transaction.
    //
    // Why not `initialize_nonce`:
    //   `NonceManagerMiddleware::initialize_nonce` is a one-time init guard — once
    //   `initialized = true` it returns the stale in-memory counter immediately, so it
    //   cannot be used to re-sync after the first call.
    //
    // Why not updating the middleware counter directly:
    //   ethers 2.x `NonceManagerMiddleware` has no public `set_nonce` method.  The
    //   internal counter (`AtomicU64`) and `initialized` flag (`AtomicBool`) are private
    //   fields with no external setter.
    //
    // How the middleware counter re-syncs after this:
    //   `NonceManagerMiddleware::send_transaction` (ethers source lines ~151-164) has its
    //   own error-recovery path: on any send failure it calls `get_transaction_count` from
    //   the chain and, if the result differs from `self.nonce.load()`, stores the fresh
    //   value and retries.  So the *next* transaction after this recovery will fail with
    //   nonce-too-low (counter M+1 vs on-chain N+1), the middleware catches it, resets its
    //   counter to N+1, and retries transparently — one extra round-trip, no data loss.
    //
    // Why pinning the nonce bypasses the middleware counter here:
    //   `NonceManagerMiddleware::fill_transaction` only calls `get_transaction_count_with_manager`
    //   (which increments the counter) when `tx.nonce().is_none()`.  An explicit nonce is
    //   left untouched, so the retry uses exactly the value fetched from the chain.
    match client
        .get_transaction_count(signer_address, Some(BlockId::Number(BlockNumber::Pending)))
        .await
    {
        Ok(fresh_nonce) => {
            function_call.tx.set_nonce(fresh_nonce);
        }
        Err(nonce_err) => {
            tracing::warn!("nonce resync failed: {nonce_err}");
            signer_pool.release(signer_address).await?;
            return Err(anyhow::anyhow!(
                "Failed to send transaction (nonce resync failed): original error: {first_err}, nonce fetch error: {nonce_err}"
            ));
        }
    }

    // Retry with the pinned nonce.
    let tx = match function_call.send().await {
        Ok(tx) => tx,
        Err(retry_err) => {
            let decoded = decode_contract_revert(&retry_err);
            signer_pool.release(signer_address).await?;
            return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
        }
    };

    let result = match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    };

    signer_pool.release(signer_address).await?;
    result
}
