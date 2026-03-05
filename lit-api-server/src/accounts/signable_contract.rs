pub use crate::abstractions::transfer::chain_info::Chain;
pub use crate::accounts::contracts::account_config_contract::AccountConfig;
use crate::config::GLOBAL_NODE_CONFIG;
use crate::dstack::v1::get_lit_payer_key;
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
use std::time::Duration;

/// The shared signing client. A single instance is held for the lifetime of
/// the process so that `NonceManagerMiddleware` can manage nonces atomically
/// across concurrent requests. Without this, every request would create a
/// fresh `SignerMiddleware`, each fetching the same pending nonce from the
/// chain and causing "nonce too low" / "replacement transaction underpriced"
/// errors under concurrency.
pub(crate) type SigningClient =
    NonceManagerMiddleware<SignerMiddleware<Provider<Http>, LocalWallet>>;

static GLOBAL_SIGNING_CLIENT: OnceLock<Arc<SigningClient>> = OnceLock::new();

static GLOBAL_READ_ONLY_CLIENT: OnceLock<Arc<Provider<Http>>> = OnceLock::new();

/// Initialise the global signing client. Must be called once at startup,
/// after `init_config()`, before any transactions are sent.
pub(crate) async fn init_chain_clients() -> Result<()> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    let secret = get_lit_payer_key(1).await.map_err(|e| anyhow::anyhow!(e))?;

    let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);
    let address = wallet.address();
    tracing::info!("API Payer wallet address: {:?}", address);
    let provider = Provider::<Http>::try_from(chain_info.rpc_url)?.interval(Duration::from_secs(2));
    let signer = SignerMiddleware::new(provider, wallet);

    // NonceManagerMiddleware wraps the signer and tracks the nonce in an
    // AtomicU64. On the first transaction it fetches the current on-chain
    // nonce; every subsequent call increments it atomically, so concurrent
    // callers always receive distinct nonces.
    let nonce_manager = NonceManagerMiddleware::new(signer, address);

    GLOBAL_SIGNING_CLIENT.get_or_init(|| Arc::new(nonce_manager));

    let provider = Provider::<Http>::try_from(chain_info.rpc_url)?.interval(Duration::from_secs(2));
    GLOBAL_READ_ONLY_CLIENT.get_or_init(|| Arc::new(provider));
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

    let client = GLOBAL_READ_ONLY_CLIENT
        .get()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Read-only client not initialised — call init_chain_clients() at startup"
            )
        })?
        .clone();
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = H160::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}
