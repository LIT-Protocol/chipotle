use ethers::core::abi::AbiDecode;

use crate::accounts::contracts::account_config_contract::AccountConfigErrors;

/// Attempt to decode a human-readable revert reason from a contract error.
///
/// Tries the following, in order:
/// 1. Contract-specific custom errors (AccountConfigErrors)
/// 2. Standard `Error(string)` / `Panic(uint256)` via lit-core
/// 3. Falls back to the raw error's `Display` output
pub fn decode_contract_revert(err: &ethers::contract::ContractError<impl ethers::providers::Middleware>) -> String {
    // Try to extract the revert data bytes from the error.
    if let Some(data) = err.as_revert() {
        // First, try contract-specific custom error decoding.
        if let Ok(decoded) = AccountConfigErrors::decode(data) {
            return format!("Contract error: {decoded}");
        }

        // Fall back to standard Error(string) / Panic(uint256).
        if let Some(reason) = lit_core::utils::decode_revert::decode_revert(data) {
            return format!("Revert: {reason}");
        }

        // Unknown custom error — show the hex.
        return format!("Unknown revert data: 0x{}", hex::encode(data));
    }

    // No revert data available — use the error's Display.
    format!("{err}")
}

