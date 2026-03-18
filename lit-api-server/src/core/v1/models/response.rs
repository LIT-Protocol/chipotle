use rocket_okapi::okapi::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NewAccountResponse {
    pub api_key: String,
    pub wallet_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HandshakeResponse {
    pub responses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateWalletResponse {
    pub wallet_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionResponse {
    pub response: serde_json::Value,
    pub logs: String,
    pub has_error: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EncryptResponse {
    pub ciphertext: String,
    pub data_to_encrypt_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecryptResponse {
    pub decrypted_text: String,
}

/// Response for account config operations (add_group, add_pkp_to_group, remove_pkp_from_group, add_usage_api_key, remove_usage_api_key).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyResponse {
    pub success: bool,
    pub usage_api_key: String,
}

/// Response for account config operations (add_group, add_pkp_to_group, remove_pkp_from_group, add_usage_api_key, remove_usage_api_key).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AccountOpResponse {
    pub success: bool,
}

/// Mirrors AccountConfig.sol Group struct (groupName, groupDescription, plus ids/hashes when returned).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GroupResponse {
    pub group_id: String,
    pub group_name: String,
    pub group_description: String,
}

/// One item from list_groups, list_wallets, list_wallets_in_group, or list_actions (AccountConfig.sol Metadata).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListMetadataItem {
    pub id: String, // hash of the item, as stored on chain.
    pub name: String,
    pub description: String,
}

/// One item from list_groups, list_wallets, list_wallets_in_group, or list_actions (AccountConfig.sol Metadata).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WalletItem {
    pub id: String, // hash of the item, as stored on chain.
    pub name: String,
    pub description: String,
    pub wallet_address: String, // if the item is managed by the LIT-node, this will be the actual IPFS CID, or Wallet Address, or public key, etc.
}

/// One item from list_api_keys (AccountConfig.sol UsageApiKey).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApiKeyItem {
    pub id: String, // hash of the item, as stored on chain.
    pub name: String,
    pub description: String,
    pub expiration: String,
    pub balance: u64,
    pub can_create_groups: bool,
    pub can_delete_groups: bool,
    pub can_create_pkps: bool,
    pub can_manage_ipfs_ids_in_groups: Vec<u64>,
    pub can_add_pkp_to_groups: Vec<u64>,
    pub can_remove_pkp_from_groups: Vec<u64>,
    pub can_execute_in_groups: Vec<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct VersionResponse {
    pub version: &'static str,
    pub src_hash: &'static str,
    pub git_commit: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeChainConfigResponse {
    pub chain_name: String,
    pub chain_id: u64,
    pub is_evm: bool,
    pub testnet: bool,
    pub token: String,
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub rpc_url: String,
    pub contract_address: String,
}
