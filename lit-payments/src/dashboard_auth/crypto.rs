//! Hashing, MACs, randomness and the outbox cipher. Wire formats match the
//! former Worker exactly (lowercase hex everywhere).

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result, anyhow};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn mac_hex(secret: &str, value: &str) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(value.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// `true` when `value` is exactly `len` bytes of lowercase hex.
pub fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len * 2
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn unhex(value: &str, len: usize) -> Result<Vec<u8>> {
    if !is_hex(value, len) {
        return Err(anyhow!("Invalid encrypted record."));
    }
    hex::decode(value).context("hex decode")
}

/// Stored password verifier: SHA-256 over the JSON array
/// `["chipotle-auth-verifier-v1", id, authSecret]`, matching the browser-side
/// definition. `auth_secret` is already an Argon2id/HKDF output, never a
/// password.
pub fn verifier(id: &str, auth_secret: &str) -> String {
    let encoded = serde_json::to_string(&["chipotle-auth-verifier-v1", id, auth_secret])
        .expect("string array serializes");
    sha256_hex(&encoded)
}

/// Deterministic fake identity/salt for unknown emails so `login/parameters`
/// cannot be used to enumerate accounts.
pub fn synthetic_id(secret: &str, email: &str) -> String {
    mac_hex(secret, &format!("identity:{email}"))[..32].to_string()
}

pub fn synthetic_salt(secret: &str, email: &str) -> String {
    mac_hex(secret, &format!("salt:{email}"))[..32].to_string()
}

fn outbox_key(secret: &str) -> Vec<u8> {
    hex::decode(mac_hex(secret, "outbox-encryption-v1")).expect("hex mac decodes")
}

/// AES-256-GCM with the outbox row id as associated data, so a payload cannot
/// be moved between rows. Returns lowercase hex ciphertext (tag appended).
pub fn encrypt_outbox(secret: &str, id: &str, nonce_hex: &str, plaintext: &[u8]) -> Result<String> {
    let key = outbox_key(secret);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = unhex(nonce_hex, 12)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| anyhow!("outbox encryption failed"))?;
    Ok(hex::encode(ciphertext))
}

pub fn decrypt_outbox(
    secret: &str,
    id: &str,
    nonce_hex: &str,
    ciphertext_hex: &str,
) -> Result<Vec<u8>> {
    let key = outbox_key(secret);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = unhex(nonce_hex, 12)?;
    if !ciphertext_hex.len().is_multiple_of(2) {
        return Err(anyhow!("Invalid encrypted record."));
    }
    let ciphertext = unhex(ciphertext_hex, ciphertext_hex.len() / 2)?;
    cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| anyhow!("outbox decryption failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_matches_browser_definition() {
        // sha256(JSON.stringify(["chipotle-auth-verifier-v1", "11"*16, "aa"*32])),
        // computed with Node's WebCrypto; guards the exact JSON encoding.
        assert_eq!(
            verifier(&"11".repeat(16), &"aa".repeat(32)),
            "eb1023da7a424b69aa78fc78fd37608319f09b663d97cd3795b92955249ba739"
        );
    }

    #[test]
    fn hex_helpers_validate_length_and_case() {
        assert!(is_hex(&"ab".repeat(12), 12));
        assert!(!is_hex(&"AB".repeat(12), 12));
        assert!(!is_hex(&"ab".repeat(11), 12));
        assert!(unhex("zz", 1).is_err());
        assert_eq!(random_hex(16).len(), 32);
        assert_ne!(random_hex(16), random_hex(16));
    }

    #[test]
    fn outbox_round_trip_binds_row_id() {
        let secret = "test-only-secret-not-for-production-12345";
        let nonce = random_hex(12);
        let ciphertext = encrypt_outbox(secret, "row-1", &nonce, b"{\"to\":\"a@b.c\"}").unwrap();
        assert_eq!(
            decrypt_outbox(secret, "row-1", &nonce, &ciphertext).unwrap(),
            b"{\"to\":\"a@b.c\"}"
        );
        assert!(decrypt_outbox(secret, "row-2", &nonce, &ciphertext).is_err());
        assert!(
            decrypt_outbox(
                "another-secret-that-is-long-enough-123456",
                "row-1",
                &nonce,
                &ciphertext
            )
            .is_err()
        );
    }

    #[test]
    fn constant_time_eq_requires_equal_length() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn synthetic_parameters_are_stable_hex() {
        let secret = "test-only-secret-not-for-production-12345";
        assert_eq!(synthetic_id(secret, "x@y.z"), synthetic_id(secret, "x@y.z"));
        assert!(is_hex(&synthetic_id(secret, "x@y.z"), 16));
        assert!(is_hex(&synthetic_salt(secret, "x@y.z"), 16));
        assert_ne!(
            synthetic_id(secret, "x@y.z"),
            synthetic_salt(secret, "x@y.z")
        );
    }
}
