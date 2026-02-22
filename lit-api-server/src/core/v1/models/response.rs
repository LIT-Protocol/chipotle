use rocket_okapi::okapi::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::actions::client::models::SignedData;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum ShareType {
    Ecdsa,
    Frost,
    Bls,
}

impl Display for ShareType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareType::Ecdsa => write!(f, "Ecdsa"),
            ShareType::Frost => write!(f, "Frost"),
            ShareType::Bls => write!(f, "Bls"),
        }
    }
}

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
    pub public_key: String,
    pub signature: String,
}

impl From<SignedData> for SignWithPkpResponse {
    fn from(signed_data: SignedData) -> Self {
        Self {
            signing_scheme: signed_data.signing_scheme,
            signed_digest: signed_data.digest,
            public_key: signed_data.public_key,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CombineSignatureSharesResponse {
    pub signature: String,
    pub signed_data: String,
    pub verifying_key: String,
    pub r: String,
    pub s: String,
    pub recovery_id: u8,
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
    pub id: String,
    pub name: String,
    pub description: String,
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