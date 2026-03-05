use crate::actions::client::models::SignedData;
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
pub struct SignWithPkpResponse {
    pub signing_scheme: String,
    pub signed_digest: String,
    pub pkp_id: String,
    pub signature: String,
}

impl From<SignedData> for SignWithPkpResponse {
    fn from(signed_data: SignedData) -> Self {
        Self {
            signing_scheme: signed_data.signing_scheme,
            signed_digest: signed_data.digest,
            pkp_id: signed_data.pkp_id,
            signature: signed_data.signature,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionSignature {
    pub name: String,
    pub data: SignWithPkpResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionResponse {
    pub signatures: Vec<LitActionSignature>,
    pub response: String,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeChainConfigResponse {
    pub chain_name: String,
    pub chain_id: u64,
    pub is_evm: bool,
    pub testnet: bool,
    pub token: String,
    pub rpc_url: String,
    pub contract_address: String,
}
