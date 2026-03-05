use super::utils::pkp_id_to_derviation_path;
use crate::actions::aes::{aes_decrypt, aes_encrypt};
use crate::dstack::v1::get_client_key;
use anyhow::{Result, anyhow};

pub async fn aes_encrypt_with_pkp(api_key: &str, pkp_id: &str, plaintext: &str) -> Result<String> {
    let derivation_path = pkp_id_to_derviation_path(api_key, pkp_id)
        .await
        .map_err(|e| anyhow!(e))?;
    let symmetric_key = get_client_key(&derivation_path)
        .await
        .map_err(|e| anyhow!(e))?;
    let encrypted = aes_encrypt(&symmetric_key, plaintext.to_string())
        .await
        .map_err(|e| anyhow!(e))?;
    Ok(encrypted)
}

pub async fn aes_decrypt_with_pkp(api_key: &str, pkp_id: &str, ciphertext: &str) -> Result<String> {
    let derivation_path = pkp_id_to_derviation_path(api_key, pkp_id)
        .await
        .map_err(|e| anyhow!(e))?;
    let symmetric_key = get_client_key(&derivation_path)
        .await
        .map_err(|e| anyhow!(e))?;
    let decrypted = aes_decrypt(&symmetric_key, ciphertext)
        .await
        .map_err(|e| anyhow!(e))?;
    Ok(decrypted)
}
