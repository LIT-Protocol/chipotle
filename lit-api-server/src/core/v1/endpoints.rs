use crate::core::internal;
use crate::core::models::ApiResult;
use crate::core::models::ErrMessage;
use crate::core::v1::models::request::{
    DecryptRequest, EncryptRequest, LitActionRequest, SignWithPKPRequest,
};
use crate::core::v1::models::response::{
    DecryptResponse, EncryptResponse, GetApiKeyResponse, HandshakeResponse, LitActionResponses,
    MintPkpResponse, SignWithPkpResponse,
};
use rocket::serde::json::Json;
use rocket::{Route, get, post, routes};
use rocket_responder::ApiResponse;

pub fn routes() -> Vec<Route> {
    routes![
        handshake,
        sign_with_pkp,
        get_api_key,
        mint_pkp,
        encrypt,
        decrypt,
        lit_action,
        get_ledger_balance
    ]
}

#[get("/get_api_key")]
async fn get_api_key() -> ApiResponse<GetApiKeyResponse, ErrMessage> {
    ApiResult(internal::get_api_key().await).into()
}

#[get("/handshake")]
async fn handshake() -> ApiResponse<HandshakeResponse, ErrMessage> {
    ApiResult(internal::handshake().await).into()
}

#[get("/mint_pkp/<api_key>")]
async fn mint_pkp(api_key: &str) -> ApiResponse<MintPkpResponse, ErrMessage> {
    ApiResult(internal::mint_pkp(api_key).await).into()
}

#[post("/sign_with_pkp", format = "json", data = "<sign_request>")]
async fn sign_with_pkp(
    sign_request: Json<SignWithPKPRequest>,
) -> ApiResponse<SignWithPkpResponse, ErrMessage> {
    ApiResult(internal::sign_with_pkp(sign_request).await).into()
}

#[post("/lit_action", format = "json", data = "<lit_action_request>")]
async fn lit_action(
    lit_action_request: Json<LitActionRequest>,
) -> ApiResponse<LitActionResponses, ErrMessage> {
    ApiResult(internal::lit_action(lit_action_request).await).into()
}

#[post("/encrypt", format = "json", data = "<encrypt_request>")]
async fn encrypt(
    encrypt_request: Json<EncryptRequest>,
) -> ApiResponse<EncryptResponse, ErrMessage> {
    ApiResult(internal::encrypt(encrypt_request).await).into()
}

#[post("/decrypt", format = "json", data = "<decrypt_request>")]
async fn decrypt(
    decrypt_request: Json<DecryptRequest>,
) -> ApiResponse<DecryptResponse, ErrMessage> {
    ApiResult(internal::decrypt(decrypt_request).await).into()
}

#[get("/get_ledger_balance/<api_key>")]
async fn get_ledger_balance(api_key: &str) -> ApiResponse<String, ErrMessage> {
    ApiResult(internal::get_ledger_balance(api_key).await).into()
}
