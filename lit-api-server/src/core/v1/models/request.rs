use serde::{Deserialize, Serialize};

use crate::core::v1::models::response::SignWithPkpResponse;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignWithPKPRequest {
    pub api_key: String,
    pub pkp_public_key: String,
    pub message: String,
    pub signing_scheme: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LitActionRequest {
    pub api_key: String,
    pub code: String,
    pub js_params: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptRequest {
    pub api_key: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecryptRequest {
    pub api_key: String,
    pub ciphertext: String,
    pub data_to_encrypt_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CombineSignatureSharesRequest {
    pub api_key: String,
    pub share_date: SignWithPkpResponse,
}
