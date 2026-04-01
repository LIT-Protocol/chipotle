use ethers::core::abi::AbiDecode;

use crate::accounts::contracts::account_config_contract::AccountConfigErrors;

/// Maximum bytes of unknown revert data to include in error messages.
const MAX_HEX_DISPLAY_BYTES: usize = 64;

/// Attempt to decode a human-readable revert reason from a contract error.
///
/// Tries the following, in order:
/// 1. Standard `Error(string)` / `Panic(uint256)` via lit-core
/// 2. Contract-specific custom errors (AccountConfigErrors)
/// 3. Falls back to the raw error's `Display` output
pub fn decode_contract_revert(
    err: &ethers::contract::ContractError<impl ethers::providers::Middleware>,
) -> String {
    // Try to extract the revert data bytes from the error.
    if let Some(data) = err.as_revert() {
        // First, try standard Error(string) / Panic(uint256) so that standard
        // revert strings are labelled consistently as "Revert: ..." rather than
        // going through AccountConfigErrors::RevertString.
        if let Some(reason) = lit_core::utils::decode_revert::decode_revert(data) {
            return format!("Revert: {reason}");
        }

        // Then try contract-specific custom error decoding.
        if let Ok(decoded) = AccountConfigErrors::decode(data) {
            return format!("Contract error: {decoded}");
        }

        // Unknown custom error — show truncated hex to avoid huge messages.
        if data.len() > MAX_HEX_DISPLAY_BYTES {
            return format!(
                "Unknown revert data ({} bytes): 0x{}...",
                data.len(),
                hex::encode(&data[..MAX_HEX_DISPLAY_BYTES])
            );
        }
        return format!("Unknown revert data: 0x{}", hex::encode(data));
    }

    // No revert data available — use the error's Display.
    format!("{err}")
}
