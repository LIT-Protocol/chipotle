use crate::actions::ActionStore;
use crate::core::internal;
use crate::core::models::ApiResult;
use crate::core::models::ErrMessage;
use crate::core::v1::models::request::{
    DecryptRequest, EncryptRequest, LitActionRequest, SignWithPKPRequest,
};
use crate::core::v1::models::response::LitActionResponse;
use crate::core::v1::models::response::{
    DecryptResponse, EncryptResponse, GetApiKeyResponse, HandshakeResponse,
    MintPkpResponse, SignWithPkpResponse,
};
use rocket::State;
use rocket::serde::json::Json;
use rocket::{Route, get, post, routes};
use rocket_responder::ApiResponse;
use crate::actions::grpc_client_pool::GrpcClientPool;
use moka::future::Cache;

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
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, String>>,
    // action_store: &State<ActionStore>,
    http_client: &State<reqwest::Client>,
    lit_action_request: Json<LitActionRequest>,
) -> ApiResponse<LitActionResponse, ErrMessage> {
    ApiResult(internal::lit_action(
        grpc_client_pool.inner(), 
        ipfs_cache.inner(), 
        // action_store.inner(),
        http_client.inner(), 
        lit_action_request
    ).await).into()
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
