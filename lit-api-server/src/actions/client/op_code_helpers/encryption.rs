use crate::actions::aes::{aes_decrypt, aes_encrypt};
use anyhow::{Result, anyhow};
use ethers::utils::keccak256;
use lit_core::utils::binary::bytes_to_hex;

pub async fn aes_encrypt_with_pkp(
    api_key: &str,
    public_key: &str,
    plaintext: &str,
) -> Result<String> {
    let personal_symmetric_key = get_personal_symmetric_key(api_key, public_key).await?;
    let encrypted = aes_encrypt(&personal_symmetric_key, plaintext.to_string()).await?;
    Ok(encrypted)
}

pub async fn aes_decrypt_with_pkp(
    api_key: &str,
    public_key: &str,
    ciphertext: &str,
) -> Result<String> {
    let personal_symmetric_key = get_personal_symmetric_key(api_key, public_key).await?;
    let decrypted = aes_decrypt(&personal_symmetric_key, ciphertext).await?;
    Ok(decrypted)
}

pub async fn get_personal_symmetric_key(api_key: &str, public_key: &str) -> Result<Vec<u8>> {
    let secret_u256 =
        match crate::accounts::get_wallet_derivation_from_pubkey(api_key, public_key).await {
            Ok(secret_u256) => secret_u256,
            Err(e) => return Err(anyhow!("Error getting wallet derivation: {:?}", e)),
        };
    if secret_u256 == ethers::types::U256::zero() {
        return Err(anyhow!("Wallet not found"));
    }
    let mut personal_symmetric_key = [0; 32];
    secret_u256.to_big_endian(&mut personal_symmetric_key);
    Ok(personal_symmetric_key.to_vec())
}

pub async fn aes_encrypt_to_action_with_pkp(
    api_key: &str,
    plaintext: &str,
    ipfs_id: &str,
) -> Result<String> {
    let symmetric_key = get_master_symmetric_key_for_action(api_key, ipfs_id).await?;
    let encrypted = aes_encrypt(&symmetric_key, plaintext.to_string()).await?;
    Ok(encrypted)
}

pub async fn aes_decrypt_to_action_with_pkp(
    api_key: &str,
    ciphertext: &str,
    ipfs_id: &str,
) -> Result<String> {
    let symmetric_key = get_master_symmetric_key_for_action(api_key, ipfs_id).await?;
    let decrypted = aes_decrypt(&symmetric_key, ciphertext).await?;
    Ok(decrypted)
}

pub async fn get_master_symmetric_key_for_action(api_key: &str, ipfs_id: &str) -> Result<Vec<u8>> {
    let master_account_api_key_hash =
        crate::accounts::get_naster_account_api_key_hash(api_key).await?;
    if master_account_api_key_hash == ethers::types::U256::zero() {
        return Err(anyhow!("Master account not found"));
    }

    let mut master_account_api_key_hash_bytes = [0; 32];
    master_account_api_key_hash.to_big_endian(&mut master_account_api_key_hash_bytes);

    let extended_key_string = format!(
        "lit-action-key-{}-{}",
        bytes_to_hex(master_account_api_key_hash_bytes),
        ipfs_id
    );
    let symmetric_key = keccak256(extended_key_string.as_bytes()).to_vec();
    Ok(symmetric_key)
}
