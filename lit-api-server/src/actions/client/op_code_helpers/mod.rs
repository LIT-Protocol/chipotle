use crate::utils::parse_with_hash::{ipfs_cid_to_u256, wallet_string_to_h160};
use anyhow::Result;
pub mod encryption;
pub mod private_keys;

pub async fn can_use_wallet_in_action(
    api_key: &str,
    ipfs_id: &str,
    wallet_address: &str,
) -> Result<bool> {
    let cid_hash = ipfs_cid_to_u256(ipfs_id)
        .map_err(|e| anyhow::anyhow!("Runner is unable to parse IPFS ID: {}", e))?;
    let wallet_address = wallet_string_to_h160(wallet_address)
        .map_err(|e| anyhow::anyhow!("Runner is unable to parse wallet address: {}", e))?;
    let can_use =
        crate::accounts::can_use_wallet_in_action(api_key, cid_hash, wallet_address).await?;
    Ok(can_use)
}

pub async fn can_use_wallet_in_action_cached(
    state: &mut crate::actions::client::models::ExecutionState,
    api_key: &str,
    ipfs_id: &str,
    wallet_address: &str,
) -> Result<bool> {
    if let Some(&cached) = state.wallet_permission_cache.get(wallet_address) {
        return Ok(cached);
    }
    let result = can_use_wallet_in_action(api_key, ipfs_id, wallet_address).await?;
    state
        .wallet_permission_cache
        .insert(wallet_address.to_string(), result);
    Ok(result)
}
