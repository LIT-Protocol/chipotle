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

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed 256-bit key for deterministic tests.
    fn test_key() -> [u8; 32] {
        [0x42u8; 32]
    }

    #[tokio::test]
    async fn roundtrip_succeeds() {
        let key = test_key();
        let plaintext = "Hello, AES-GCM!".to_string();
        let ciphertext = aes_encrypt(&key, plaintext.clone()).await.unwrap();
        let decrypted = aes_decrypt(&key, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn tamper_detection() {
        let key = test_key();
        let ciphertext_hex = aes_encrypt(&key, "secret".to_string()).await.unwrap();
        // Flip a byte in the ciphertext portion (after the 12-byte nonce).
        let mut bytes = hex_to_bytes(&ciphertext_hex).unwrap();
        bytes[12] ^= 0xFF;
        let tampered = bytes_to_hex(bytes);
        let result = aes_decrypt(&key, &tampered).await;
        assert!(result.is_err(), "tampered ciphertext should fail to decrypt");
    }

    #[tokio::test]
    async fn short_input_rejected() {
        let key = test_key();
        // 27 bytes is less than the required minimum of 12 (nonce) + 16 (GCM tag) = 28 bytes.
        let short_hex = "00".repeat(27);
        let result = aes_decrypt(&key, &short_hex).await;
        assert!(result.is_err(), "input shorter than nonce+tag should be rejected");
    }

    #[tokio::test]
    async fn wrong_key_fails() {
        let key1 = [0x11u8; 32];
        let key2 = [0x22u8; 32];
        let ciphertext = aes_encrypt(&key1, "secret".to_string()).await.unwrap();
        let result = aes_decrypt(&key2, &ciphertext).await;
        assert!(result.is_err(), "decryption with wrong key should fail");
    }
}
