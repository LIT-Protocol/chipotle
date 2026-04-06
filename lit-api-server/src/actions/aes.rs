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
        return Err(validation_err("Invalid ciphertext: too short", None));
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

    fn test_key() -> [u8; 32] {
        [0x42u8; 32]
    }

    #[tokio::test]
    async fn encrypt_decrypt_round_trip() {
        let key = test_key();
        let plaintext = "Hello, world!".to_string();
        let ciphertext = aes_encrypt(&key, plaintext.clone()).await.unwrap();
        let decrypted = aes_decrypt(&key, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn encrypt_produces_hex_output() {
        let key = test_key();
        let ciphertext = aes_encrypt(&key, "test".to_string()).await.unwrap();
        // bytes_to_hex produces hex without "0x" prefix
        assert!(ciphertext.chars().all(|c| c.is_ascii_hexdigit()));
        // Nonce (12) + plaintext (4) + auth tag (16) = 32 bytes = 64 hex chars minimum
        assert!(ciphertext.len() >= 64);
    }

    #[tokio::test]
    async fn encrypt_is_nondeterministic() {
        let key = test_key();
        let ct1 = aes_encrypt(&key, "same".to_string()).await.unwrap();
        let ct2 = aes_encrypt(&key, "same".to_string()).await.unwrap();
        assert_ne!(ct1, ct2, "each encryption should use a random nonce");
    }

    #[tokio::test]
    async fn decrypt_wrong_key_fails() {
        let key1 = test_key();
        let key2 = [0x43u8; 32];
        let ciphertext = aes_encrypt(&key1, "secret".to_string()).await.unwrap();
        let result = aes_decrypt(&key2, &ciphertext).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn decrypt_too_short_ciphertext() {
        let key = test_key();
        // Less than 28 bytes (12 nonce + 16 auth tag)
        let short_hex = "0x0102030405060708090a0b0c";
        let result = aes_decrypt(&key, short_hex).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn decrypt_invalid_hex() {
        let key = test_key();
        let result = aes_decrypt(&key, "not_hex_at_all").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn encrypt_empty_string_produces_undecryptable_ciphertext() {
        let key = test_key();
        let ciphertext = aes_encrypt(&key, String::new()).await.unwrap();
        // Empty string encryption succeeds (nonce + auth tag, no payload bytes).
        assert!(ciphertext.len() >= 56);
        // However, aes_decrypt rejects empty decryption results (line 26-31), so
        // round-tripping an empty string is expected to fail.
        let result = aes_decrypt(&key, &ciphertext).await;
        assert!(
            result.is_err(),
            "empty plaintext should fail round-trip decryption"
        );
    }

    #[tokio::test]
    async fn encrypt_decrypt_unicode() {
        let key = test_key();
        let plaintext = "Hello 🌍 Héllo".to_string();
        let ciphertext = aes_encrypt(&key, plaintext.clone()).await.unwrap();
        let decrypted = aes_decrypt(&key, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn decrypt_tampered_ciphertext_fails() {
        let key = test_key();
        let ciphertext = aes_encrypt(&key, "secret data".to_string()).await.unwrap();
        // Flip a byte in the ciphertext portion (after the 12-byte nonce = 24 hex chars)
        let mut tampered = ciphertext.clone().into_bytes();
        let flip_pos = 24; // first byte after nonce in hex
        tampered[flip_pos] ^= 0x01;
        let tampered = String::from_utf8(tampered).unwrap();
        let result = aes_decrypt(&key, &tampered).await;
        assert!(
            result.is_err(),
            "tampered ciphertext should be rejected by GCM auth tag"
        );
    }

    #[tokio::test]
    async fn invalid_key_length() {
        let short_key = [0u8; 16]; // AES-256 requires 32 bytes
        let result = aes_encrypt(&short_key, "test".to_string()).await;
        assert!(result.is_err());
    }
}
