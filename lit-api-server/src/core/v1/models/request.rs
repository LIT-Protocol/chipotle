use serde::{Deserialize, Serialize};
use rocket_okapi::okapi::schemars::JsonSchema;
use crate::core::v1::models::response::SignWithPkpResponse;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NewAccountRequest {
    pub account_name: String,
    pub account_description: String,
    /// Optional initial balance for the account (AccountConfig.accountApiKey.balance). Decimal or hex string; default 0.
    #[serde(default)]
    pub initial_balance: Option<String>,
}

/// Request for add_group. permitted_actions and pkps are keccak256 hashes as hex strings (with or without 0x).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddGroupRequest {
    pub api_key: String,
    /// Name of the group (Group.metadata.name in AccountConfig.sol).
    pub group_name: String,
    /// Description of the group (Group.metadata.description in AccountConfig.sol).
    pub group_description: String,
    /// Keccak256 hashes of action IPFS CIDs (hex strings).
    pub permitted_actions: Vec<String>,
    /// Keccak256 hashes of PKP/wallet public keys (hex strings).
    pub pkps: Vec<String>,
    /// If true, all wallets are permitted to use the group (AccountConfig.sol Group.all_wallets_permitted).
    #[serde(default)]
    pub all_wallets_permitted: bool,
    /// If true, all actions are permitted (AccountConfig.sol Group.all_actions_permitted).
    #[serde(default)]
    pub all_actions_permitted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddActionToGroupRequest {
    pub api_key: String,
    pub group_id: String,
    /// IPFS CID for the action (will be keccak256-hashed on server).
    pub action_ipfs_cid: String,
    /// Optional name for the action (stored in contract metadata).
    pub name: Option<String>,
    /// Optional description for the action (stored in contract metadata).
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddPkpToGroupRequest {
    pub api_key: String,
    /// Group ID (decimal or hex string).
    pub group_id: String,
    pub pkp_public_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemovePkpFromGroupRequest {
    pub api_key: String,
    pub group_id: String,
    pub pkp_public_key: String,
}

/// Request for update_group (AccountConfig.updateGroup).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateGroupRequest {
    pub api_key: String,
    /// Group ID (decimal or hex string).
    pub group_id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub all_wallets_permitted: bool,
    #[serde(default)]
    pub all_actions_permitted: bool,
}

/// Request for remove_action_from_group. action_ipfs_cid is keccak256-hashed on server.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveActionFromGroupRequest {
    pub api_key: String,
    pub group_id: String,
    /// IPFS CID for the action (keccak256-hashed on server).
    pub action_ipfs_cid: String,
}

/// Request for update_action_metadata. action_ipfs_cid is keccak256-hashed on server.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateActionMetadataRequest {
    pub api_key: String,
    pub group_id: String,
    /// IPFS CID for the action (keccak256-hashed on server).
    pub action_ipfs_cid: String,
    pub name: String,
    pub description: String,
}

/// Request for update_usage_api_key_metadata (AccountConfig.updateUsageApiKeyMetadata).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateUsageApiKeyMetadataRequest {
    pub api_key: String,
    pub usage_api_key: String,
    pub name: String,
    pub description: String,
}

/// Request for add_usage_api_key. expiration and balance as decimal strings (e.g. unix timestamp, wei).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyRequest {
    pub api_key: String,
    pub usage_api_key: String,
    pub expiration: String,
    pub balance: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveUsageApiKeyRequest {
    pub api_key: String,
    pub usage_api_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SignWithPKPRequest {
    pub api_key: String,
    pub pkp_public_key: String,
    pub message: String,
    pub signing_scheme: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionRequest {
    pub api_key: String,
    pub code: String,
    pub js_params: Option<serde_json::Value>,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CombineSignatureSharesRequest {
    pub api_key: String,
    pub share_date: SignWithPkpResponse,
}
