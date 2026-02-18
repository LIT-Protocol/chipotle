pub mod contracts;
pub use contracts::account_config::AccountConfig;
pub mod signable_contract;
pub use anyhow::Result;

use crate::accounts::signable_contract::get_signable_account_config_contract;
use ethers::{types::U256, utils::keccak256};

const ACCOUNT_CONFIG_CONTRACT_ADDRESS: &str = "0xdca98b7c113496fdc59c654a01571b8a3b427d2b";
const ACCOUNT_CONFIG_SIGNER_PRIVATE_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

pub async fn new_account(api_key: &str) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let api_key = U256::from_big_endian(&keccak256(api_key));
    contract.new_account(api_key, true).send().await?;
    Ok(true)
}
