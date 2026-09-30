use crate::actions::aes::{aes_decrypt, aes_encrypt};
use crate::core::get_verified_client_key;
use anyhow::{Result, anyhow};
use tracing::instrument;

#[instrument(skip_all, err)]
pub async fn aes_encrypt_with_pkp(api_key: &str, pkp_id: &str, plaintext: &str) -> Result<String> {
    // Verify the symmetric key belongs to pkp_id before using it, so a caller
    // can't encrypt under (and later decrypt) another wallet's key via an
    // aliased derivation path.
    let symmetric_key = get_verified_client_key(api_key, pkp_id)
        .await
        .map_err(|e| anyhow!(e))?;
    let encrypted = aes_encrypt(&symmetric_key, plaintext.to_string())
        .await
        .map_err(|e| anyhow!(e))?;
    Ok(encrypted)
}

#[instrument(skip_all, err)]
pub async fn aes_decrypt_with_pkp(api_key: &str, pkp_id: &str, ciphertext: &str) -> Result<String> {
    // Verify the symmetric key belongs to pkp_id before using it, so a caller
    // can't decrypt another user's ciphertext via an aliased derivation path.
    let symmetric_key = get_verified_client_key(api_key, pkp_id)
        .await
        .map_err(|e| anyhow!(e))?;
    let decrypted = aes_decrypt(&symmetric_key, ciphertext)
        .await
        .map_err(|e| anyhow!(e))?;
    Ok(decrypted)
}
