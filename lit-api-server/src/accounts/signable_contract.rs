pub use crate::abstractions::transfer::chain_info::Chain;
pub use crate::accounts::contracts::account_config::AccountConfig;
use crate::config::GLOBAL_NODE_CONFIG;
pub use anyhow::Result;
use ethers::middleware::NonceManagerMiddleware;
pub use ethers::middleware::SignerMiddleware;
pub use ethers::providers::Http;
pub use ethers::providers::Provider;
pub use ethers::signers::LocalWallet;
use ethers::signers::Signer;
pub use ethers::types::H160;
pub use lit_core::utils::binary::hex_to_bytes;
pub use std::sync::Arc;
use std::sync::OnceLock;

/// The shared signing client. A single instance is held for the lifetime of
/// the process so that `NonceManagerMiddleware` can manage nonces atomically
/// across concurrent requests. Without this, every request would create a
/// fresh `SignerMiddleware`, each fetching the same pending nonce from the
/// chain and causing "nonce too low" / "replacement transaction underpriced"
/// errors under concurrency.
pub(crate) type SigningClient =
    NonceManagerMiddleware<SignerMiddleware<Provider<Http>, LocalWallet>>;

static GLOBAL_SIGNING_CLIENT: OnceLock<Arc<SigningClient>> = OnceLock::new();

/// Initialise the global signing client. Must be called once at startup,
/// after `init_config()`, before any transactions are sent.
pub(crate) fn init_signing_client() -> Result<()> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    let secret = hex_to_bytes(&node_config.secret)?;

    let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);
    let address = wallet.address();
    let provider = Provider::<Http>::try_from(chain_info.rpc_url)?;
    let signer = SignerMiddleware::new(provider, wallet);

    // NonceManagerMiddleware wraps the signer and tracks the nonce in an
    // AtomicU64. On the first transaction it fetches the current on-chain
    // nonce; every subsequent call increments it atomically, so concurrent
    // callers always receive distinct nonces.
    let nonce_manager = NonceManagerMiddleware::new(signer, address);

    GLOBAL_SIGNING_CLIENT.get_or_init(|| Arc::new(nonce_manager));
    Ok(())
}

pub(crate) async fn get_signable_account_config_contract()
-> Result<AccountConfig<SigningClient>, anyhow::Error> {
    let client = GLOBAL_SIGNING_CLIENT
        .get()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Signing client not initialised — call init_signing_client() at startup"
            )
        })?
        .clone();

    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = H160::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}


pub(crate) async fn get_read_only_account_config_contract()
-> Result<AccountConfig<Provider<Http>>, anyhow::Error> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;

    let provider = Provider::<Http>::try_from(node_config.chain.info().rpc_url)?;
    let client = Arc::new(provider);
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = H160::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}
