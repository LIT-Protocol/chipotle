//! Local key derivation.
//!
//! The TEE derives a fresh 32-byte secret per `(purpose, path)` from a
//! hardware-held root key (`dstack` v1: a keccak256 over the raw derived
//! key), then feeds that secret straight into a secp256k1 signer or an
//! AES-256 key. We reproduce the *shape* of that scheme against a
//! developer-supplied local master key: deterministic, domain-separated,
//! and always a valid secp256k1 scalar so every command agrees on the key
//! for a given id.
//!
//! Values will NOT match production (different root key) — the point is that
//! the CLI *surface* is identical, so action code developed here runs
//! unchanged in the TEE.

use alloy::primitives::keccak256;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context as _, Result};

/// Namespace tags mirroring the TEE key purposes (`dstack::v1`).
const PURPOSE_CLIENT: &str = "client";
const PURPOSE_ACTION: &str = "lit_action";

/// Derive a valid secp256k1 secret from `(master, purpose, path)`.
///
/// keccak256 over a domain-separated preimage, retrying with an appended
/// counter on the astronomically unlikely chance the 32 bytes are not a
/// valid scalar (out of range / zero). Mirrors the TEE, which likewise
/// treats its 32-byte derived secret as a secp256k1 key.
fn derive_secret(master: &[u8; 32], purpose: &str, path: &str) -> [u8; 32] {
    for counter in 0u8..=u8::MAX {
        let mut preimage = Vec::with_capacity(master.len() + purpose.len() + path.len() + 3);
        preimage.extend_from_slice(master);
        preimage.push(0);
        preimage.extend_from_slice(purpose.as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(path.as_bytes());
        preimage.push(counter);

        let secret = keccak256(&preimage).0;
        if PrivateKeySigner::from_slice(&secret).is_ok() {
            return secret;
        }
    }
    // 256 consecutive invalid scalars is not reachable in practice.
    unreachable!("failed to derive a valid secp256k1 scalar for {purpose}/{path}");
}

/// The per-PKP secret, used both as the wallet private key and as the
/// AES-256 symmetric key (TEE parity: `get_client_key` serves both).
pub fn pkp_secret(master: &[u8; 32], pkp_id: &str) -> [u8; 32] {
    derive_secret(master, PURPOSE_CLIENT, pkp_id)
}

/// This action's own secret, keyed by its content id.
pub fn action_secret(master: &[u8; 32], ipfs_id: &str) -> [u8; 32] {
    derive_secret(master, PURPOSE_ACTION, ipfs_id)
}

/// The action's own secret, matching the TEE's `lit_action_<cid>` domain.
pub fn action_signer(master: &[u8; 32], ipfs_id: &str) -> Result<PrivateKeySigner> {
    signer(&action_secret(master, ipfs_id))
}

pub fn signer(secret: &[u8; 32]) -> Result<PrivateKeySigner> {
    PrivateKeySigner::from_slice(secret).context("secret bytes are not a valid secp256k1 key")
}

/// Compressed SEC1 public key bytes, matching the TEE's
/// `verifying_key().to_sec1_bytes()`.
pub fn public_key_bytes(signer: &PrivateKeySigner) -> Vec<u8> {
    signer.credential().verifying_key().to_sec1_bytes().to_vec()
}
