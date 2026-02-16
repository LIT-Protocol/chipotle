use crate::core::models::ApiStatus;
use crate::core::v1::models::request::{
    DecryptRequest, EncryptRequest, LitActionRequest, SignWithPKPRequest,    
};
use crate::core::v1::models::response::{
    DecryptResponse, EncryptResponse, GetApiKeyResponse,
    HandshakeResponse, LitActionResponses, MintPkpResponse, SignWithPkpResponse,
};
use rocket::serde::json::Json;

fn not_configured() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Lit testnet not configured"),
        "This operation is not available.",
    )
}

pub async fn get_api_key() -> Result<GetApiKeyResponse, ApiStatus> {
    Err(not_configured())
}

pub async fn handshake() -> Result<HandshakeResponse, ApiStatus> {
    Err(not_configured())
}

pub async fn get_ledger_balance(api_key: &str) -> Result<String, ApiStatus> {
    let _ = api_key;
    Err(not_configured())
}

pub async fn mint_pkp(api_key: &str) -> Result<MintPkpResponse, ApiStatus> {
    let _ = api_key;
    Err(not_configured())
}

pub async fn sign_with_pkp(
    sign_request: Json<SignWithPKPRequest>,
) -> Result<SignWithPkpResponse, ApiStatus> {
    let _ = sign_request;
    Err(not_configured())
}

pub async fn lit_action(
    lit_action_request: Json<LitActionRequest>,
) -> Result<LitActionResponses, ApiStatus> {
    let _ = lit_action_request;
    Err(not_configured())
}

pub async fn encrypt(encrypt_request: Json<EncryptRequest>) -> Result<EncryptResponse, ApiStatus> {
    let _ = encrypt_request;
    Err(not_configured())
}

pub async fn decrypt(decrypt_request: Json<DecryptRequest>) -> Result<DecryptResponse, ApiStatus> {
    let _ = decrypt_request;
    Err(not_configured())
}
