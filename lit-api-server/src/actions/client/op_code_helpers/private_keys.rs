use crate::{
    core::pkp_id_to_derviation_path,
    dstack::v1::{get_client_key, get_lit_action_key},
};
use anyhow::Result;
use ethers::signers::{LocalWallet, Signer};
use lit_core::utils::binary::bytes_to_0x_hex;

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

pub async fn get_lit_action_private_key(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action key for private key: {e}"))?;
    let secret = bytes_to_0x_hex(secret_bytes);
    Ok(secret)
}

pub async fn get_lit_action_public_key(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action public key: {e}"))?;
    let local_wallet = LocalWallet::from_bytes(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Unable to convert secret bytes to local wallet: {e}"))?;
    let public_key_bytes = local_wallet
        .signer()
        .verifying_key()
        .to_encoded_point(false);
    let public_key = bytes_to_0x_hex(public_key_bytes.to_bytes());
    Ok(public_key)
}

pub async fn get_lit_action_wallet_address(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action wallet address: {e}"))?;
    let local_wallet = LocalWallet::from_bytes(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Unable to convert secret bytes to local wallet: {e}"))?;
    let wallet_address = local_wallet.address();
    let wallet_address = format!("{:?}", wallet_address);
    Ok(wallet_address)
}
