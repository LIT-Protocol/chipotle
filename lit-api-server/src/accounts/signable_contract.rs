pub use crate::abstractions::transfer::chain_info::Chain;
pub use crate::accounts::contracts::account_config::AccountConfig;
use crate::accounts::{ACCOUNT_CONFIG_CONTRACT_ADDRESS, ACCOUNT_CONFIG_SIGNER_PRIVATE_KEY};
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
    let chain = Chain::Yellowstone;
    let chain_info = chain.info();
    let secret = ACCOUNT_CONFIG_SIGNER_PRIVATE_KEY;
    let secret = hex_to_bytes(secret)?;

    let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);

    let provider = Provider::<Http>::try_from(chain_info.rpc_url)?;
    let signing_provider = SignerMiddleware::new(provider.clone(), wallet);

    let client = Arc::new(signing_provider);
    let account_config_address = hex_to_bytes(ACCOUNT_CONFIG_CONTRACT_ADDRESS)?;
    let account_config_address = H160::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}
