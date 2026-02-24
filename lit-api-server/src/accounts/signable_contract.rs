pub use crate::abstractions::transfer::chain_info::Chain;
pub use crate::accounts::contracts::account_config::AccountConfig;
use crate::config::GLOBAL_NODE_CONFIG;
pub use anyhow::Result;
pub use ethers::middleware::SignerMiddleware;
pub use ethers::providers::Http;
pub use ethers::providers::Provider;
pub use ethers::signers::LocalWallet;
use ethers::signers::Signer;
pub use ethers::types::H160;
pub use lit_core::utils::binary::hex_to_bytes;
pub use std::sync::Arc;

pub(crate) async fn get_signable_account_config_contract()
-> Result<AccountConfig<SignerMiddleware<Provider<Http>, LocalWallet>>, anyhow::Error> {
    // let chain = Chain::Yellowstone;
    // let chain = Chain::Anvil;
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or(anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    let secret = hex_to_bytes(&node_config.secret)?;

    let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);

    let provider = Provider::<Http>::try_from(chain_info.rpc_url)?;
    let signing_provider = SignerMiddleware::new(provider.clone(), wallet);

    let client = Arc::new(signing_provider);
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = H160::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}
