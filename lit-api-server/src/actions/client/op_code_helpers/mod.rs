use crate::utils::parse_with_hash::{ipfs_cid_to_u256, wallet_string_to_h160};
use anyhow::Result;
use tracing::instrument;
pub mod encryption;
pub mod private_keys;

/// Check every wallet operation through the shared blockchain cache. Do not
/// memoize permissions in ExecutionState: that would bypass the shared cache's
/// TTL and selective invalidation by API mutations and the account-event listener.
#[instrument(
    name = "op::can_use_wallet_in_action",
    level = "debug",
    skip(api_key),
    err
)]
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
