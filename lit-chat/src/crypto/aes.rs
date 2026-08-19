//! AES-256-GCM with Additional Authenticated Data.
//!
//! Based on `lit-api-server/src/actions/aes.rs`, extended per
//! plans/tee-chat-app.md section 4.3: every ciphertext in the chat store is
//! bound to its location via AAD, so a DB-level attacker who swaps rows,
//! reorders via seq, or flips a role produces bytes that fail decryption.
//!
//! Wire format: `nonce[12] || ciphertext || tag[16]`, raw bytes (BYTEA).
//! AAD is transported out-of-band — it is recomputed from the row's own
//! plaintext coordinates at decrypt time, which is the whole point.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{anyhow, Result};
use rand::RngCore;

pub fn encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("could not create cipher: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| anyhow!("encryption failed"))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    // At least a 12-byte nonce and a 16-byte GCM tag.
    if data.len() < 12 + 16 {
        return Err(anyhow!("invalid ciphertext: too short"));
    }
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("could not create cipher: {e}"))?;
    let nonce = Nonce::from_slice(&data[0..12]);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &data[12..],
                aad,
            },
        )
        // Deliberately unspecific: wrong key, wrong AAD, and tampered bytes
        // are indistinguishable to callers and to logs.
        .map_err(|_| anyhow!("decryption failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] {
        [7u8; 32]
    }

    #[test]
    fn roundtrip() {
        let ct = encrypt(&key(), b"hello", b"aad-1").unwrap();
        assert_eq!(decrypt(&key(), &ct, b"aad-1").unwrap(), b"hello");
    }

    #[test]
    fn wrong_aad_fails() {
        let ct = encrypt(&key(), b"hello", b"aad-1").unwrap();
        assert!(decrypt(&key(), &ct, b"aad-2").is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let mut ct = encrypt(&key(), b"hello", b"aad-1").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 1;
        assert!(decrypt(&key(), &ct, b"aad-1").is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt(&key(), b"hello", b"aad-1").unwrap();
        assert!(decrypt(&[8u8; 32], &ct, b"aad-1").is_err());
    }

    #[test]
    fn nonces_are_unique_per_call() {
        let a = encrypt(&key(), b"hello", b"aad").unwrap();
        let b = encrypt(&key(), b"hello", b"aad").unwrap();
        assert_ne!(a[..12], b[..12]);
    }

    #[test]
    fn too_short_rejected() {
        assert!(decrypt(&key(), &[0u8; 27], b"aad").is_err());
    }
}
