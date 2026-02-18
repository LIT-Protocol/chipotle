use crate::core::internal;
use crate::core::models::ApiResult;
use crate::core::models::ErrMessage;
use crate::core::v1::models::request::{
    LitActionRequest, SignWithPKPRequest,
};
use crate::core::v1::models::response::CreateWalletResponse;
use crate::core::v1::models::response::LitActionResponse;
use crate::core::v1::models::response::{
    GetApiKeyResponse,
     SignWithPkpResponse,
};
use rocket::State;
use rocket::serde::json::Json;
use rocket::{Route, get, post, routes};
use rocket_responder::ApiResponse;
use crate::actions::grpc_client_pool::GrpcClientPool;
use moka::future::Cache;

pub fn routes() -> Vec<Route> {
    routes![
        sign_with_pkp,
        get_api_key,
        create_wallet,
        lit_action,
    ]
}

#[get("/get_api_key")]
async fn get_api_key() -> ApiResponse<GetApiKeyResponse, ErrMessage> {
    ApiResult(internal::get_api_key().await).into()
}

#[get("/create_wallet/<api_key>")]
async fn create_wallet(api_key: &str) -> ApiResponse<CreateWalletResponse, ErrMessage> {
    ApiResult(internal::create_wallet(api_key).await).into()
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

