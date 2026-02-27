use crate::error::{Result, conversion_err, validation_err};
use libaes::Cipher;
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};
use rand::Rng;

pub async fn aes_decrypt(symmetric_key: &[u8], ciphertext_with_iv: &str) -> Result<String> {
    // Create a new 256-bit cipher
    let user_key = symmetric_key.try_into().map_err(|e| {
        conversion_err(
            "Could not convert symmetric key to length 32",
            Some(format!("{}", e)),
        )
    })?;
    let cipher = Cipher::new_256(&user_key);

    let ciphertext_with_iv = hex_to_bytes(ciphertext_with_iv)?;
    if ciphertext_with_iv.len() < 16 {
        return Err(validation_err("Ciphertext is too short", None));
    }

    let iv = &ciphertext_with_iv[0..16];
    let ciphertext = &ciphertext_with_iv[16..];

    // CBC ciphertext must be a multiple of block size (16)
    if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return Err(validation_err(
            "Invalid ciphertext length (must be non-empty and multiple of 16 bytes)",
            None,
        ));
    }

    let decrypted = cipher.cbc_decrypt(iv, ciphertext);

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
    let user_key = symmetric_key.try_into().map_err(|e| {
        conversion_err(
            "Could not convert symmetric key to length 32",
            Some(format!("{}", e)),
        )
    })?;
    let cipher = Cipher::new_256(&user_key);

    // get random byte slice that is 16 bytes long
    let mut iv = [0; 16];
    rand::thread_rng().fill(&mut iv);
    let plaintext_bytes = plaintext.as_bytes();
    let encrypted = cipher.cbc_encrypt(&iv, plaintext_bytes);
    // Prepend IV to ciphertext
    let mut result = iv.to_vec();
    result.extend_from_slice(&encrypted);
    Ok(bytes_to_hex(result))
}
