use crate::error::{Result, conversion_err, validation_err};
use libaes::Cipher;
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};
use rand::Rng;

pub async fn aes_decrypt(symmetric_key: &[u8], ciphertext_with_iv: &str) -> Result<String> {
    // Create a new 128-bit cipher
    let cipher_bytes = symmetric_key
        .try_into()
        .map_err(|e| conversion_err("Could not convert symmetric key to length 32", None))?;
    let cipher = Cipher::new_256(&cipher_bytes);

    
    let ciphertext_with_iv = hex_to_bytes(ciphertext_with_iv)?;
    if ciphertext_with_iv.len() < 16 {
        return Err(validation_err("Ciphertext is too short", None));
    }


    let iv = &ciphertext_with_iv[0..16];
    let ciphertext = &ciphertext_with_iv[16..];
    // Decryption
    let decrypted = cipher.cbc_decrypt(iv, ciphertext);

    Ok(bytes_to_hex(decrypted))
}


pub async fn aes_encrypt(symmetric_key: Vec<u8>, plaintext: String) -> Result<String> {
    let cipher_bytes = symmetric_key
        .try_into()
        .map_err(|e| conversion_err("Could not convert symmetric key to length 32", None))?;
    let cipher = Cipher::new_256(&cipher_bytes);

    // get random byte slice that is 16 bytes long
    let mut iv = [0; 16];
    rand::thread_rng().fill(&mut iv);
    let encrypted = cipher.cbc_encrypt(&iv, plaintext.as_bytes());
    // Prepend IV to ciphertext
    let mut result = iv.to_vec();
    result.extend_from_slice(&encrypted);
    Ok(bytes_to_hex(result))
}

}