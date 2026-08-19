//! Envelope encryption (plans/tee-chat-app.md section 4.3).
//!
//! User KEK (enclave-derived, on demand) wraps a random per-conversation
//! DEK; the DEK encrypts messages, titles, and usage metadata. Migration
//! (anon -> account) rewraps N DEKs instead of re-encrypting all messages.
//!
//! AAD canonical forms (versioned, pipe-delimited; every component is either
//! a UUID, a hex hash, an integer, or a role token, so the encoding is
//! unambiguous):
//!   message   "chat.msg.v1|{conversation_id}|{message_id}|{seq}|{role}"
//!   dek wrap  "chat.dek.v1|{user_ref_hash}|{conversation_id}"
//!   title     "chat.title.v1|{conversation_id}"
//!   usage     "chat.usage.v1|{conversation_id}|{message_id}"
//!   meter     "chat.meter.v1|{user_ref_hash}"

use crate::crypto::aes;
use anyhow::{anyhow, Result};
use rand::RngCore;
use uuid::Uuid;

pub fn mint_dek() -> [u8; 32] {
    let mut dek = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut dek);
    dek
}

pub fn wrap_dek(
    kek: &[u8; 32],
    dek: &[u8; 32],
    user_ref_hash: &str,
    conversation_id: Uuid,
) -> Result<Vec<u8>> {
    let aad = format!("chat.dek.v1|{user_ref_hash}|{conversation_id}");
    aes::encrypt(kek, dek, aad.as_bytes())
}

pub fn unwrap_dek(
    kek: &[u8; 32],
    wrapped: &[u8],
    user_ref_hash: &str,
    conversation_id: Uuid,
) -> Result<[u8; 32]> {
    let aad = format!("chat.dek.v1|{user_ref_hash}|{conversation_id}");
    let dek = aes::decrypt(kek, wrapped, aad.as_bytes())?;
    dek.try_into()
        .map_err(|_| anyhow!("unwrapped DEK has wrong length"))
}

pub fn encrypt_message(
    dek: &[u8; 32],
    conversation_id: Uuid,
    message_id: Uuid,
    seq: i64,
    role: &str,
    content: &str,
) -> Result<Vec<u8>> {
    let aad = format!("chat.msg.v1|{conversation_id}|{message_id}|{seq}|{role}");
    aes::encrypt(dek, content.as_bytes(), aad.as_bytes())
}

pub fn decrypt_message(
    dek: &[u8; 32],
    conversation_id: Uuid,
    message_id: Uuid,
    seq: i64,
    role: &str,
    ciphertext: &[u8],
) -> Result<String> {
    let aad = format!("chat.msg.v1|{conversation_id}|{message_id}|{seq}|{role}");
    let bytes = aes::decrypt(dek, ciphertext, aad.as_bytes())?;
    String::from_utf8(bytes).map_err(|_| anyhow!("message plaintext is not utf8"))
}

pub fn encrypt_title(dek: &[u8; 32], conversation_id: Uuid, title: &str) -> Result<Vec<u8>> {
    let aad = format!("chat.title.v1|{conversation_id}");
    aes::encrypt(dek, title.as_bytes(), aad.as_bytes())
}

pub fn decrypt_title(dek: &[u8; 32], conversation_id: Uuid, ciphertext: &[u8]) -> Result<String> {
    let aad = format!("chat.title.v1|{conversation_id}");
    let bytes = aes::decrypt(dek, ciphertext, aad.as_bytes())?;
    String::from_utf8(bytes).map_err(|_| anyhow!("title plaintext is not utf8"))
}

pub fn encrypt_usage_meta(
    dek: &[u8; 32],
    conversation_id: Uuid,
    message_id: Uuid,
    json: &str,
) -> Result<Vec<u8>> {
    let aad = format!("chat.usage.v1|{conversation_id}|{message_id}");
    aes::encrypt(dek, json.as_bytes(), aad.as_bytes())
}

pub fn encrypt_meter(kek: &[u8; 32], user_ref_hash: &str, json: &str) -> Result<Vec<u8>> {
    let aad = format!("chat.meter.v1|{user_ref_hash}");
    aes::encrypt(kek, json.as_bytes(), aad.as_bytes())
}

pub fn decrypt_meter(kek: &[u8; 32], user_ref_hash: &str, ciphertext: &[u8]) -> Result<String> {
    let aad = format!("chat.meter.v1|{user_ref_hash}");
    let bytes = aes::decrypt(kek, ciphertext, aad.as_bytes())?;
    String::from_utf8(bytes).map_err(|_| anyhow!("meter plaintext is not utf8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_bound_to_coordinates() {
        let dek = mint_dek();
        let conv = Uuid::new_v4();
        let msg = Uuid::new_v4();
        let ct = encrypt_message(&dek, conv, msg, 3, "user", "hi").unwrap();
        assert_eq!(
            decrypt_message(&dek, conv, msg, 3, "user", &ct).unwrap(),
            "hi"
        );
        // Any moved / re-labeled ciphertext fails decryption:
        assert!(decrypt_message(&dek, Uuid::new_v4(), msg, 3, "user", &ct).is_err());
        assert!(decrypt_message(&dek, conv, Uuid::new_v4(), 3, "user", &ct).is_err());
        assert!(decrypt_message(&dek, conv, msg, 4, "user", &ct).is_err());
        assert!(decrypt_message(&dek, conv, msg, 3, "assistant", &ct).is_err());
    }

    #[test]
    fn dek_wrap_bound_to_owner_and_conversation() {
        let kek = [1u8; 32];
        let dek = mint_dek();
        let conv = Uuid::new_v4();
        let wrapped = wrap_dek(&kek, &dek, "hash-a", conv).unwrap();
        assert_eq!(unwrap_dek(&kek, &wrapped, "hash-a", conv).unwrap(), dek);
        assert!(unwrap_dek(&kek, &wrapped, "hash-b", conv).is_err());
        assert!(unwrap_dek(&kek, &wrapped, "hash-a", Uuid::new_v4()).is_err());
    }

    #[test]
    fn rewrap_flow() {
        // Anon -> account migration: unwrap with old KEK/hash, wrap with new.
        let old_kek = [1u8; 32];
        let new_kek = [2u8; 32];
        let dek = mint_dek();
        let conv = Uuid::new_v4();
        let wrapped = wrap_dek(&old_kek, &dek, "anon-hash", conv).unwrap();
        let dek2 = unwrap_dek(&old_kek, &wrapped, "anon-hash", conv).unwrap();
        let rewrapped = wrap_dek(&new_kek, &dek2, "acct-hash", conv).unwrap();
        assert_eq!(
            unwrap_dek(&new_kek, &rewrapped, "acct-hash", conv).unwrap(),
            dek
        );
    }
}
