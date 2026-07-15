//! Hex helpers matching `lit_core::utils::binary` byte-for-byte, so the
//! CLI's output encoding is identical to the TEE runner's.

use anyhow::{Context as _, Result};

/// Lowercase hex, no `0x` prefix (matches `bytes_to_hex`).
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Lowercase hex with a `0x` prefix (matches `bytes_to_0x_hex`).
pub fn bytes_to_0x_hex(bytes: &[u8]) -> String {
    format!("0x{}", bytes_to_hex(bytes))
}

/// Decode hex, tolerating a leading `0x` and an odd length (matches
/// `hex_to_bytes`: an odd string is left-padded with a `0` nibble).
pub fn hex_to_bytes(input: &str) -> Result<Vec<u8>> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    let mut owned = stripped.to_string();
    if owned.len() % 2 == 1 {
        owned.insert(0, '0');
    }
    hex::decode(&owned).context("failed to decode hex from str")
}
