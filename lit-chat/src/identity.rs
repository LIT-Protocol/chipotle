//! User identity (plans/tee-chat-app.md section 5).
//!
//! - Anonymous: a random 128-bit `user_ref` minted in-enclave. The session
//!   cookie carrying it is a bearer capability — it IS the key-derivation
//!   input; there is no server-side account to reset.
//! - Account: `user_ref = HKDF(user-id-namespace, lower(email))`. The
//!   derivation IS the lookup: a returning user resolves to their existing
//!   row without the DB ever holding an email, a hash of one, or a UUID
//!   mapped to one.
//! - `user_ref_hash = keccak(user_ref)`; the raw ref never touches the DB.

use crate::crypto::keccak256;
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserKind {
    Anon,
    Account,
}

impl UserKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserKind::Anon => "anon",
            UserKind::Account => "account",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "anon" => Some(UserKind::Anon),
            "account" => Some(UserKind::Account),
            _ => None,
        }
    }
}

/// Mint a fresh anonymous ref: "anon:" + 32 hex chars (128 bits).
pub fn anon_user_ref() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("anon:{}", hex::encode(bytes))
}

/// Derive the account ref for an email. Deterministic per namespace key:
/// the email address is the permanent identity anchor (section 5.3 — an
/// address change is a migration, not a profile edit).
pub fn account_user_ref(namespace: &[u8; 32], email: &str) -> String {
    let email = email.trim().to_lowercase();
    let hk = Hkdf::<Sha256>::new(Some(namespace), email.as_bytes());
    let mut out = [0u8; 16];
    hk.expand(b"chat/v1/user-ref", &mut out)
        .expect("16 bytes is a valid HKDF-SHA256 output length");
    format!("acct:{}", hex::encode(out))
}

/// Opaque DB key for a ref. Hex keccak256 of the ref string.
pub fn user_ref_hash(user_ref: &str) -> String {
    hex::encode(keccak256(user_ref.as_bytes()))
}

pub fn kind_of_ref(user_ref: &str) -> Option<UserKind> {
    if user_ref.starts_with("anon:") {
        Some(UserKind::Anon)
    } else if user_ref.starts_with("acct:") {
        Some(UserKind::Account)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_ref_is_deterministic_and_case_insensitive() {
        let ns = [9u8; 32];
        let a = account_user_ref(&ns, "Alice@Example.com");
        let b = account_user_ref(&ns, "alice@example.com ");
        assert_eq!(a, b);
        assert!(a.starts_with("acct:"));
    }

    #[test]
    fn different_namespace_different_ref() {
        let a = account_user_ref(&[1u8; 32], "a@b.c");
        let b = account_user_ref(&[2u8; 32], "a@b.c");
        assert_ne!(a, b);
    }

    #[test]
    fn anon_refs_are_unique() {
        assert_ne!(anon_user_ref(), anon_user_ref());
    }

    #[test]
    fn hash_is_stable_hex() {
        let h = user_ref_hash("anon:00ff");
        assert_eq!(h.len(), 64);
        assert_eq!(h, user_ref_hash("anon:00ff"));
    }
}
