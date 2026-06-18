use alloy::primitives::keccak256;
use alloy::sol_types::SolInterface;

use crate::accounts::contracts::account_config_contract::AccountConfigErrors;

/// Maximum bytes of unknown revert data to include in error messages.
const MAX_HEX_DISPLAY_BYTES: usize = 64;

const ACCOUNT_CONFIG_ERROR_SIGNATURES: &[(&str, &str)] = &[
    ("AccountAlreadyExists", "AccountAlreadyExists(uint256)"),
    ("AccountDoesNotExist", "AccountDoesNotExist(uint256)"),
    (
        "ActionDoesNotExist",
        "ActionDoesNotExist(uint256,uint256,uint256)",
    ),
    ("GroupDoesNotExist", "GroupDoesNotExist(uint256,uint256)"),
    (
        "InsufficientBalance",
        "InsufficientBalance(uint256,uint256)",
    ),
    ("InvalidRequest", "InvalidRequest(string)"),
    ("NoAccountAccess", "NoAccountAccess(uint256,address)"),
    (
        "NotAllowedToAddPkpToGroup",
        "NotAllowedToAddPkpToGroup(uint256,uint256)",
    ),
    (
        "NotAllowedToCreateGroup",
        "NotAllowedToCreateGroup(uint256)",
    ),
    ("NotAllowedToCreatePkp", "NotAllowedToCreatePkp(uint256)"),
    (
        "NotAllowedToDeleteGroup",
        "NotAllowedToDeleteGroup(uint256)",
    ),
    (
        "NotAllowedToManageIPFSIdsInGroup",
        "NotAllowedToManageIPFSIdsInGroup(uint256,uint256)",
    ),
    (
        "NotAllowedToRemovePkpFromGroup",
        "NotAllowedToRemovePkpFromGroup(uint256,uint256)",
    ),
    ("NotContractOwner", "NotContractOwner(address,address)"),
    ("NotMasterAccount", "NotMasterAccount(uint256)"),
    ("OnlyApiPayerOrOwner", "OnlyApiPayerOrOwner(address)"),
    (
        "OnlyApiPayerOrPricingOperator",
        "OnlyApiPayerOrPricingOperator(address)",
    ),
    (
        "OnlyConfigOperatorOrOwner",
        "OnlyConfigOperatorOrOwner(address)",
    ),
    (
        "PkpDoesNotExist",
        "PkpDoesNotExist(uint256,uint256,address)",
    ),
    (
        "UsageApiKeyDoesNotExist",
        "UsageApiKeyDoesNotExist(uint256,uint256)",
    ),
];

fn account_config_error_name(data: &[u8]) -> Option<&'static str> {
    let selector = data.get(..4)?;
    ACCOUNT_CONFIG_ERROR_SIGNATURES
        .iter()
        .find(|(_, signature)| &keccak256(signature.as_bytes()).as_slice()[..4] == selector)
        .map(|(name, _)| *name)
}

/// Attempt to decode a human-readable revert reason from an alloy contract error.
pub fn decode_contract_revert(err: &alloy::contract::Error) -> String {
    if let Some(data) = err.as_revert_data() {
        // First, try standard Error(string) / Panic(uint256) so that standard
        // revert strings are labelled consistently as "Revert: ..." rather than
        // going through AccountConfigErrors::RevertString.
        if let Some(reason) = lit_core::utils::decode_revert::decode_revert(&data) {
            return format!("Revert: {reason}");
        }

        if AccountConfigErrors::abi_decode(&data).is_ok() {
            let name = account_config_error_name(&data).unwrap_or("AccountConfigError");
            return format!("Contract error: {name} (0x{})", hex::encode(data));
        }

        if data.len() > MAX_HEX_DISPLAY_BYTES {
            return format!(
                "Unknown revert data ({} bytes): 0x{}...",
                data.len(),
                hex::encode(&data[..MAX_HEX_DISPLAY_BYTES])
            );
        }
        return format!("Unknown revert data: 0x{}", hex::encode(data));
    }

    format!("{err}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolError;

    use crate::accounts::contracts::account_config_contract::AccountConfig;

    /// The `AccountDoesNotExist` selector must resolve to its name. Billing's
    /// `wallet_resolution_err` substring-matches on this name to return a 400
    /// (account not found) rather than a 500, so a selector/name drift here
    /// would silently turn missing-account lookups back into 500s.
    #[test]
    fn account_does_not_exist_selector_resolves_to_name() {
        let selector = AccountConfig::AccountDoesNotExist::SELECTOR;
        assert_eq!(
            account_config_error_name(&selector),
            Some("AccountDoesNotExist")
        );
    }

    #[test]
    fn unknown_selector_resolves_to_none() {
        assert_eq!(account_config_error_name(&[0x00, 0x00, 0x00, 0x00]), None);
    }
}
