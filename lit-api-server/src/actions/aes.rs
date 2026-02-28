use crate::error::{Result, validation_err};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};
use rand::Rng;

pub async fn aes_decrypt(symmetric_key: &[u8], ciphertext_with_nonce: &str) -> Result<String> {
    // Create a new 256-bit cipher

    let cipher = Aes256Gcm::new_from_slice(symmetric_key)
        .map_err(|e| validation_err("Could not create cipher", Some(format!("{}", e))))?;

    let ciphertext_with_nonce = hex_to_bytes(ciphertext_with_nonce)?;

    // Require at least 12 bytes for the nonce and 16 bytes for the GCM authentication tag
    if ciphertext_with_nonce.len() < 12 + 16 {
        return Err(validation_err(
            "Invalid ciphertext: too short",
            None,
        ));
    }
    let nonce = Nonce::from_slice(&ciphertext_with_nonce[0..12]);
    let ciphertext = &ciphertext_with_nonce[12..];

    let decrypted = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| validation_err("Decryption failed", Some(format!("{}", e))))?;

    // libaes returns an empty Vec on decryption failure (wrong key, bad padding, tampered data)
    if decrypted.is_empty() {
        return Err(validation_err(
            "Decryption failed (invalid key or corrupted ciphertext)",
            None,
        ));
    }

    let plaintext = String::from_utf8_lossy(&decrypted).to_string();
    Ok(plaintext)
}

pub async fn aes_encrypt(symmetric_key: &[u8], plaintext: String) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(symmetric_key)
        .map_err(|e| validation_err("Could not create cipher", Some(format!("{}", e))))?;
    // 96 bit nonce
    let mut nonce = [0; 12];
    rand::thread_rng().fill(&mut nonce);
    let nonce = Nonce::from_slice(&nonce);
    let plaintext_bytes = plaintext.as_bytes();
    let encrypted = cipher
        .encrypt(nonce, plaintext_bytes)
        .map_err(|e| validation_err("Encryption failed", Some(format!("{}", e))))?;
    let mut result = nonce.to_vec();
    result.extend_from_slice(&encrypted);
    Ok(bytes_to_hex(result))
}
