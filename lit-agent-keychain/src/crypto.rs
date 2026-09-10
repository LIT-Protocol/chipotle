//! Authenticated encryption for scoped Chipotle usage API keys.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use rand::RngCore;

pub const NONCE_LEN: usize = 12;

pub fn encrypt_usage_key(master_key: &[u8], usage_key: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let cipher = cipher(master_key)?;
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), usage_key.as_bytes())
        .map_err(|_| anyhow::anyhow!("usage key encryption failed"))?;
    Ok((nonce.to_vec(), ciphertext))
}

pub fn decrypt_usage_key(master_key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<String> {
    if nonce.len() != NONCE_LEN {
        anyhow::bail!("invalid usage key nonce length");
    }
    let cipher = cipher(master_key)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow::anyhow!("usage key decryption failed"))?;
    String::from_utf8(plaintext).context("usage key plaintext is not UTF-8")
}

fn cipher(master_key: &[u8]) -> Result<Aes256Gcm> {
    if master_key.len() < 32 {
        anyhow::bail!("USAGE_KEY_ENCRYPTION_KEY must decode to at least 32 bytes");
    }
    Aes256Gcm::new_from_slice(&master_key[..32]).context("initializing usage key cipher")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";
    const WRONG: &[u8] = b"abcdef0123456789abcdef0123456789";

    #[test]
    fn roundtrip_usage_key() {
        let (nonce, ciphertext) = encrypt_usage_key(KEY, "lit_usage_sk_secret").unwrap();
        assert_ne!(ciphertext, b"lit_usage_sk_secret");
        let plain = decrypt_usage_key(KEY, &nonce, &ciphertext).unwrap();
        assert_eq!(plain, "lit_usage_sk_secret");
    }

    #[test]
    fn rejects_wrong_key() {
        let (nonce, ciphertext) = encrypt_usage_key(KEY, "lit_usage_sk_secret").unwrap();
        assert!(decrypt_usage_key(WRONG, &nonce, &ciphertext).is_err());
    }

    #[test]
    fn rejects_tampering() {
        let (nonce, mut ciphertext) = encrypt_usage_key(KEY, "lit_usage_sk_secret").unwrap();
        ciphertext[0] ^= 0x01;
        assert!(decrypt_usage_key(KEY, &nonce, &ciphertext).is_err());
    }

    #[test]
    fn rejects_short_key() {
        assert!(encrypt_usage_key(b"short", "secret").is_err());
    }
}
