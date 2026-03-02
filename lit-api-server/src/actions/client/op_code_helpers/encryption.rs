use crate::actions::aes::{aes_decrypt, aes_encrypt};
use anyhow::{Result, anyhow};

pub async fn aes_encrypt_with_pkp(
    api_key: &str,
    public_key: &str,
    plaintext: &str,
) -> Result<String> {
    let secret_u256 =
        match crate::accounts::get_wallet_derivation_from_pubkey(api_key, public_key).await {
            Ok(secret_u256) => secret_u256,
            Err(e) => return Err(anyhow!("Error getting wallet derivation: {:?}", e)),
        };

    if secret_u256 == ethers::types::U256::zero() {
        return Err(anyhow!("Wallet not found"));
    }
    let mut symmetric_key = [0; 32];
    secret_u256.to_big_endian(&mut symmetric_key);

    let encrypted = aes_encrypt(&symmetric_key, plaintext.to_string()).await?;
    Ok(encrypted)
}

pub async fn aes_decrypt_with_pkp(
    api_key: &str,
    public_key: &str,
    ciphertext: &str,
) -> Result<String> {
    let secret_u256 =
        match crate::accounts::get_wallet_derivation_from_pubkey(api_key, public_key).await {
            Ok(secret_u256) => secret_u256,
            Err(e) => return Err(anyhow!("Error getting wallet derivation: {:?}", e)),
        };
    if secret_u256 == ethers::types::U256::zero() {
        return Err(anyhow!("Wallet not found"));
    }
    let mut symmetric_key = [0; 32];
    secret_u256.to_big_endian(&mut symmetric_key);

    let decrypted = aes_decrypt(&symmetric_key, ciphertext).await?;
    Ok(decrypted)
}
