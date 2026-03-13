use rocket_okapi::okapi::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NewAccountRequest {
    pub account_name: String,
    pub account_description: String,
}

/// Request for add_group. permitted_actions and pkps are keccak256 hashes as hex strings (with or without 0x). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddGroupRequest {
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
    pub group_id: u64,
    /// IPFS CID for the action (will be keccak256-hashed on server).
    pub action_ipfs_cid: String,
    /// Optional name for the action (stored in contract metadata).
    pub name: Option<String>,
    /// Optional description for the action (stored in contract metadata).
    pub description: Option<String>,
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

/// Request for update_group (AccountConfig.updateGroup). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateGroupRequest {
    /// Group ID (decimal or hex string).
    pub group_id: u64,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub all_wallets_permitted: bool,
    #[serde(default)]
    pub all_actions_permitted: bool,
}

/// Request for remove_action_from_group. action_ipfs_cid is keccak256-hashed on server. API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveActionFromGroupRequest {
    pub group_id: u64,
    /// IPFS CID for the action (keccak256-hashed on server).
    pub action_ipfs_cid: String,
}

/// Request for update_action_metadata. action_ipfs_cid is keccak256-hashed on server. API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateActionMetadataRequest {
    pub group_id: u64,
    /// IPFS CID for the action (keccak256-hashed on server).
    pub action_ipfs_cid: String,
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

/// Request for add_usage_api_key. expiration and balance as decimal strings (e.g. unix timestamp, wei). API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyRequest {
    pub name: String,
    pub description: String,
    pub can_create_groups: bool,
    pub can_delete_groups: bool,
    pub can_create_pkps: bool,
    /// Group IDs, where 0 is the wildcard for all groups.
    pub can_manage_ipfs_ids_in_groups: Vec<u64>,    
    /// Group IDs, where 0 is the wildcard for all groups.
    pub can_add_pkp_to_groups: Vec<u64>,
    /// Group IDs, where 0 is the wildcard for all groups.
    pub can_remove_pkp_from_groups: Vec<u64>,
    /// Group IDs, where 0 is the wildcard for all groups.
    pub can_execute_in_groups: Vec<u64>,
}

/// API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveUsageApiKeyRequest {
    pub usage_api_key: String,
}

/// API key via header.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionRequest {
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
