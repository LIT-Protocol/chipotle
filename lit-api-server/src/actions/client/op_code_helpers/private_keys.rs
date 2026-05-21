use crate::{
    core::pkp_id_to_derviation_path,
    dstack::v1::{get_client_key, get_lit_action_key},
};
use alloy::signers::local::PrivateKeySigner;
use anyhow::Result;
use lit_core::utils::binary::bytes_to_0x_hex;
use tracing::instrument;

#[instrument(skip_all, err)]
pub async fn get_private_key(api_key: &str, pkp_id: &str) -> Result<String> {
    let derivation_path = pkp_id_to_derviation_path(api_key, pkp_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get derivation path for private key: {e}"))?;
    let secret_bytes = get_client_key(&derivation_path)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get client key for private key: {e}"))?;
    let secret = bytes_to_0x_hex(secret_bytes);
    Ok(secret)
}

#[instrument(skip_all, err)]
pub async fn get_lit_action_private_key(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action key for private key: {e}"))?;
    let secret = bytes_to_0x_hex(secret_bytes);
    Ok(secret)
}

#[instrument(skip_all, err)]
pub async fn get_lit_action_public_key(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action public key: {e}"))?;
    let signer = PrivateKeySigner::from_slice(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Unable to convert secret bytes to PrivateKeySigner: {e}"))?;
    let public_key_bytes = signer.credential().verifying_key().to_sec1_bytes();
    let public_key = bytes_to_0x_hex(&public_key_bytes);
    Ok(public_key)
}

#[instrument(skip_all, err)]
pub async fn get_lit_action_wallet_address(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action wallet address: {e}"))?;
    let signer = PrivateKeySigner::from_slice(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Unable to convert secret bytes to PrivateKeySigner: {e}"))?;
    let wallet_address = signer.address();
    Ok(bytes_to_0x_hex(wallet_address.as_slice()))
}
