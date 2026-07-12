//! AES-256-GCM, byte-compatible with the TEE runner (`actions::aes`): a
//! random 12-byte nonce is prepended to the ciphertext and the whole thing
//! is emitted as hex (no `0x`). `aes_decrypt` reverses it and tolerates a
//! `0x` prefix / odd length via [`hexutil::hex_to_bytes`].

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Result, anyhow, bail};
use rand::Rng as _;

use crate::hexutil;

/// 12-byte GCM nonce + 16-byte auth tag.
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

pub fn aes_encrypt(key: &[u8; 32], plaintext: &str) -> Result<String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("could not create cipher: {e}"))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("encryption failed: {e}"))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(hexutil::bytes_to_hex(&result))
}

pub fn aes_decrypt(key: &[u8; 32], ciphertext_with_nonce: &str) -> Result<String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("could not create cipher: {e}"))?;

    let bytes = hexutil::hex_to_bytes(ciphertext_with_nonce)?;
    if bytes.len() < NONCE_LEN + TAG_LEN {
        bail!("invalid ciphertext: too short");
    }

    let nonce = Nonce::from_slice(&bytes[..NONCE_LEN]);
    let decrypted = cipher
        .decrypt(nonce, &bytes[NONCE_LEN..])
        .map_err(|_| anyhow!("decryption failed (invalid key or corrupted ciphertext)"))?;

    // TEE parity: lit-api-server rejects an empty decrypted payload
    // (`actions::aes`), so an empty-plaintext ciphertext is undecryptable
    // there. Match that here so `aes-encrypt ""` doesn't "work locally,
    // break in the TEE".
    if decrypted.is_empty() {
        bail!("decryption failed (invalid key or corrupted ciphertext)");
    }

    Ok(String::from_utf8_lossy(&decrypted).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = [0x42u8; 32];
        let ct = aes_encrypt(&key, "Hello, world!").unwrap();
        assert!(
            ct.chars().all(|c| c.is_ascii_hexdigit()),
            "hex, no 0x: {ct}"
        );
        assert_eq!(aes_decrypt(&key, &ct).unwrap(), "Hello, world!");
    }

    #[test]
    fn wrong_key_fails() {
        let ct = aes_encrypt(&[0x42u8; 32], "secret").unwrap();
        assert!(aes_decrypt(&[0x43u8; 32], &ct).is_err());
    }

    #[test]
    fn nonce_makes_output_nondeterministic() {
        let key = [7u8; 32];
        assert_ne!(
            aes_encrypt(&key, "same").unwrap(),
            aes_encrypt(&key, "same").unwrap()
        );
    }

    #[test]
    fn empty_plaintext_is_undecryptable_matching_the_tee() {
        // The TEE rejects an empty decrypted payload; encrypting "" must not
        // round-trip locally either.
        let key = [9u8; 32];
        let ct = aes_encrypt(&key, "").unwrap();
        assert!(aes_decrypt(&key, &ct).is_err());
    }
}
