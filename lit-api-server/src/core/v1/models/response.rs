use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetApiKeyResponse {
    pub api_key: String,
    pub wallet_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub responses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MintPkpResponse {
    pub pkp_public_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignatureShare {
    pub share_id: String,
    pub peer_id: String,
    pub signature_share: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignWithPkpResponse {
    pub signing_scheme: String,
    pub signed_digest: String,
    pub public_key: String,
    pub share_type: ShareType,
    pub big_r: Option<String>,
    pub compressed_public_key: Option<String>,
    pub verifying_share: Option<String>,
    pub signing_commitments: Option<String>,
    pub shares: Vec<SignatureShare>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LitActionResponses {
    pub responses: Vec<LitActionResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LitActionResponse {
    pub signatures: Vec<SignWithPkpResponse>,
    pub response: String,
    pub logs: String,
    pub has_error: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptResponse {
    pub ciphertext: String,
    pub data_to_encrypt_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecryptResponse {
    pub decrypted_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CombineSignatureSharesResponse {
    pub signature: String,
    pub signed_data: String,
    pub verifying_key: String,
    pub r: String,
    pub s: String,
    pub recovery_id: u8,
}
