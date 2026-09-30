use rocket_okapi::okapi::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NewAccountRequest {
    pub account_name: String,
    pub account_description: String,
    /// Optional email address — forwarded to Stripe for the customer record.  Not stored on-chain.
    #[serde(default)]
    pub email: Option<String>,
}

/// Body for `convert_to_chain_secured_account`. The caller is authenticated by their
/// existing API key (header). The supplied wallet becomes the on-chain admin and the
/// account flips from managed to ChainSecured. The conversion is irreversible.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConvertToChainSecuredAccountRequest {
    /// Hex-encoded EVM address (with or without 0x prefix). Must be the wallet
    /// the user controls; verified by an EIP-712 typed-data signature.
    pub new_admin_wallet_address: String,
    /// EIP-712 typed-data object the wallet signed. Must use
    /// `primaryType: "ConvertAccount"` and the canonical schema (see
    /// `core::eip712`).
    pub typed_data: serde_json::Value,
    /// 65-byte 0x-prefixed signature (r||s||v) over the EIP-712 digest of
    /// `typed_data`.
    pub signature: String,
}

/// Request for add_group. permitted_actions and pkps are keccak256 hashes as hex strings (with or without 0x). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddGroupRequest {
    /// Name of the group (Group.metadata.name in AccountConfig.sol).
    pub group_name: String,
    /// Description of the group (Group.metadata.description in AccountConfig.sol).
    pub group_description: String,
    /// pkp ids permitted to use the group (AccountConfig.sol Group.pkpId).
    pub pkp_ids_permitted: Vec<String>,
    /// Actions permitted to use the group (AccountConfig.sol Group.cidHash).
    pub cid_hashes_permitted: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddActionRequest {
    /// IPFS CID for the action (keccak256-hashed on server).
    pub action_ipfs_cid: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddActionToGroupRequest {
    pub group_id: u64,
    /// IPFS CID for the action (will be keccak256-hashed on server).
    pub action_ipfs_cid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddPkpToGroupRequest {
    /// Group ID (decimal or hex string).
    pub group_id: u64,
    pub pkp_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemovePkpFromGroupRequest {
    pub group_id: u64,
    pub pkp_id: String,
}

/// Request for delete_wallet (AccountConfig.removeWalletDerivation). Master (account) API
/// key via header — usage API keys are rejected on-chain (`NotMasterAccount`).
///
/// HARD DELETE: permanently and irreversibly removes the wallet (PKP) and wipes its
/// on-chain derivation path. Anything secured by the wallet becomes unrecoverable.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteWalletRequest {
    /// Wallet (PKP) address to permanently delete: 20-byte hex, with an optional
    /// `0x`/`0X` prefix.
    pub wallet_address: String,
}

/// Request for update_group (AccountConfig.updateGroup). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateGroupRequest {
    /// Group ID (decimal or hex string).
    pub group_id: u64,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub pkp_ids_permitted: Vec<String>,
    #[serde(default)]
    pub cid_hashes_permitted: Vec<String>,
}

/// Request for delete_action. hashed_cid is already a keccak256 hash (hex string). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteActionRequest {
    /// Already-hashed CID for the action (0x-prefixed hex string).
    pub hashed_cid: String,
}

/// Request for remove_action_from_group. hashed_cid is already a keccak256 hash (hex string). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveActionFromGroupRequest {
    pub group_id: u64,
    /// Already-hashed CID for the action (0x-prefixed hex string).
    pub hashed_cid: String,
}

/// Request for update_action_metadata. hashed_cid is already a keccak256 hash (hex string). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateActionMetadataRequest {
    /// Already-hashed CID for the action (0x-prefixed hex string).
    pub hashed_cid: String,
    pub name: String,
    pub description: String,
}

/// Request for update_usage_api_key_metadata (AccountConfig.updateUsageApiKeyMetadata). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateUsageApiKeyMetadataRequest {
    pub usage_api_key: String,
    pub name: String,
    pub description: String,
}

/// Request for update_usage_api_key. Updates all permissions and metadata on an existing usage API key. API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateUsageApiKeyRequest {
    pub usage_api_key: String,
    pub name: String,
    pub description: String,
    pub can_create_groups: bool,
    pub can_delete_groups: bool,
    pub can_create_pkps: bool,
    /// Group IDs to grant manage-IPFS-IDs permission. 0 is wildcard for all groups.
    pub manage_ipfs_ids_in_groups: Vec<u64>,
    /// Group IDs to grant add-PKP permission. 0 is wildcard for all groups.
    pub add_pkp_to_groups: Vec<u64>,
    /// Group IDs to grant remove-PKP permission. 0 is wildcard for all groups.
    pub remove_pkp_from_groups: Vec<u64>,
    /// Group IDs to grant execute permission. 0 is wildcard for all groups.
    pub execute_in_groups: Vec<u64>,
}

/// Request for add_usage_api_key. expiration and balance as decimal strings (e.g. unix timestamp, wei). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyRequest {
    pub name: String,
    pub description: String,
    pub can_create_groups: bool,
    pub can_delete_groups: bool,
    pub can_create_pkps: bool,
    /// Group IDs to grant manage-IPFS-IDs permission. 0 is wildcard for all groups.
    pub manage_ipfs_ids_in_groups: Vec<u64>,
    /// Group IDs to grant add-PKP permission. 0 is wildcard for all groups.
    pub add_pkp_to_groups: Vec<u64>,
    /// Group IDs to grant remove-PKP permission. 0 is wildcard for all groups.
    pub remove_pkp_from_groups: Vec<u64>,
    /// Group IDs to grant execute permission. 0 is wildcard for all groups.
    pub execute_in_groups: Vec<u64>,
}

/// API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveUsageApiKeyRequest {
    pub usage_api_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveGroupRequest {
    pub group_id: String,
}

/// API key via header.
///
/// Provide either `code` (inline JS) or `ipfs_id` (IPFS CID of a previously-cached action).
/// When `code` is provided it is cached by its IPFS hash so subsequent calls can use `ipfs_id`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionRequest {
    /// Inline JS source. Optional when `ipfs_id` is supplied.
    #[serde(default)]
    pub code: Option<String>,
    /// IPFS CID of a previously-submitted action. Looked up in the in-memory cache.
    #[serde(default)]
    pub ipfs_id: Option<String>,
    pub js_params: Option<serde_json::Value>,
}

/// POST /lit_binary_action
///
/// Executes an any-language action **bundle** in the gVisor runner. Provide
/// either `bundle` (a base64-encoded tar/tar.gz of payload files) or
/// `checksum` (the content id of a bundle the runner already cached). When
/// `bundle` is supplied the server derives the checksum from the decoded tar
/// bytes and authorizes on that derived value — a client-supplied `checksum`
/// is only a hint and is ignored if it disagrees.
///
/// The sandbox only ever executes `bash startup.sh` (CPL-355): the
/// `startup_script` sent here, or the `startup.sh` at the bundle root.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitBinaryActionRequest {
    /// Base64-encoded tar or tar.gz bundle. Optional when `checksum` refers to
    /// a previously-submitted bundle the runner still has cached.
    #[serde(default)]
    pub bundle: Option<String>,
    /// Content id (IPFS CID) of the bundle. Required when `bundle` is omitted;
    /// when `bundle` is present it is only a hint, validated against the value
    /// derived from the bundle bytes.
    #[serde(default)]
    pub checksum: Option<String>,
    /// Bash script executed as the sandbox entrypoint (`bash startup.sh`).
    /// Sent separately from `bundle` so different scripts reuse the same
    /// cached bundle. Optional when the bundle ships a `startup.sh` at its
    /// root; the request-supplied script wins when both exist.
    #[serde(default)]
    pub startup_script: Option<String>,
    /// Parameters passed to the action: exposed to guest code via `lit params`,
    /// and top-level values are injected into the sandbox environment.
    pub js_params: Option<serde_json::Value>,
}

/// POST /billing/create_payment_intent
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreatePaymentIntentRequest {
    /// Amount to charge in US cents (minimum 500 = $5.00).
    pub amount_cents: i64,
}

/// POST /billing/confirm_payment
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConfirmPaymentRequest {
    pub payment_intent_id: String,
}

/// ChainSecured wallet creation. The client signs EIP-712 typed data with
/// their wallet; the server verifies the signature, mints a PKP via DStack
/// MPC, and returns the new wallet address + derivation path so the client
/// can register it on-chain via `registerWalletDerivation`.
///
/// The server does not maintain a nonce store. Replay protection is a
/// ±5-minute window on the `issuedAt` field plus the per-flow primaryType
/// binding — worst-case replay still just mints an extra unregistered PKP
/// (compute cost only; registration requires a separate wallet signature
/// on-chain).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateWalletWithSignatureRequest {
    /// EIP-712 typed-data object the wallet signed. Must use
    /// `primaryType: "CreateWallet"` and the canonical schema (see
    /// `core::eip712`).
    pub typed_data: serde_json::Value,
    /// 65-byte 0x-prefixed signature (r||s||v) over the EIP-712 digest of
    /// `typed_data`.
    pub signature: String,
}

/// ChainSecured usage-key minting. Mirrors `CreateWalletWithSignatureRequest`:
/// the user proves wallet ownership with an EIP-712 typed-data signature,
/// the server mints a usage-key wallet via DStack MPC and returns the
/// secret (as the usage API key) plus address + derivation path. The
/// client follows up with on-chain `registerWalletDerivation` and
/// `setUsageApiKey` signed by their admin wallet — only the admin wallet
/// of a ChainSecured account can call `setUsageApiKey` (see
/// AppStorage.accountExistsAndIsMutable).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyWithSignatureRequest {
    /// EIP-712 typed-data object the wallet signed. Must use
    /// `primaryType: "AddUsageApiKey"` and the canonical schema (see
    /// `core::eip712`).
    pub typed_data: serde_json::Value,
    /// 65-byte 0x-prefixed signature (r||s||v) over the EIP-712 digest of
    /// `typed_data`.
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EncryptRequest {
    pub api_key: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecryptRequest {
    pub api_key: String,
    pub ciphertext: String,
    pub data_to_encrypt_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_lit_action_request_with_code_only() {
        let json = r#"{"code": "console.log('hi')"}"#;
        let req: LitActionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.code, Some("console.log('hi')".to_string()));
        assert_eq!(req.ipfs_id, None);
        assert_eq!(req.js_params, None);
    }

    #[test]
    fn deserialize_lit_action_request_with_ipfs_id_only() {
        let json = r#"{"ipfs_id": "QmTest123"}"#;
        let req: LitActionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.code, None);
        assert_eq!(req.ipfs_id, Some("QmTest123".to_string()));
    }

    #[test]
    fn deserialize_lit_action_request_with_both() {
        let json = r#"{"code": "1+1", "ipfs_id": "QmTest", "js_params": {"a": 1}}"#;
        let req: LitActionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.code, Some("1+1".to_string()));
        assert_eq!(req.ipfs_id, Some("QmTest".to_string()));
        assert!(req.js_params.is_some());
    }

    #[test]
    fn deserialize_lit_action_request_with_neither() {
        let json = r#"{}"#;
        let req: LitActionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.code, None);
        assert_eq!(req.ipfs_id, None);
    }

    #[test]
    fn deserialize_lit_action_request_null_code_is_none() {
        let json = r#"{"code": null}"#;
        let req: LitActionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.code, None);
    }
}
