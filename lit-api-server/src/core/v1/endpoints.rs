use crate::actions::grpc_client_pool::GrpcClientPool;
use crate::core::internal;
use crate::core::models::ApiResult;
use crate::core::models::ErrMessage;
use crate::core::v1::models::request::{
    AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest, AddUsageApiKeyRequest,
    LitActionRequest, NewAccountRequest, RemovePkpFromGroupRequest, RemoveUsageApiKeyRequest,
    SignWithPKPRequest,
};
use crate::core::v1::models::response::{
    AccountOpResponse, CreateWalletResponse, ListMetadataItem, LitActionResponse,
    NewAccountResponse, SignWithPkpResponse,
};
use moka::future::Cache;
use rocket::State;
use rocket::serde::json::Json;
use rocket::{Route, get, post, routes};
use rocket_responder::ApiResponse;

pub fn routes() -> Vec<Route> {
    routes![
        sign_with_pkp,
        new_account,
        create_wallet,
        lit_action,
        add_group,
        add_action_to_group,
        add_pkp_to_group,
        remove_pkp_from_group,
        add_usage_api_key,
        remove_usage_api_key,
        list_groups,
        list_wallets,
        list_wallets_in_group,
        list_actions,
    ]
}

#[post("/new_account", format = "json", data = "<new_account_request>")]
async fn new_account(
    new_account_request: Json<NewAccountRequest>,
) -> ApiResponse<NewAccountResponse, ErrMessage> {
    ApiResult(internal::new_account(new_account_request).await).into()
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
    ApiResult(
        internal::lit_action(
            grpc_client_pool.inner(),
            ipfs_cache.inner(),
            // action_store.inner(),
            http_client.inner(),
            lit_action_request,
        )
        .await,
    )
    .into()
}

#[post("/add_group", format = "json", data = "<req>")]
async fn add_group(req: Json<AddGroupRequest>) -> ApiResponse<AccountOpResponse, ErrMessage> {
    ApiResult(internal::add_group(req).await).into()
}

#[post("/add_action_to_group", format = "json", data = "<req>")]
async fn add_action_to_group(
    req: Json<AddActionToGroupRequest>,
) -> ApiResponse<AccountOpResponse, ErrMessage> {
    ApiResult(internal::add_action_to_group(req).await).into()
}

#[post("/add_pkp_to_group", format = "json", data = "<req>")]
async fn add_pkp_to_group(
    req: Json<AddPkpToGroupRequest>,
) -> ApiResponse<AccountOpResponse, ErrMessage> {
    ApiResult(internal::add_pkp_to_group(req).await).into()
}

#[post("/remove_pkp_from_group", format = "json", data = "<req>")]
async fn remove_pkp_from_group(
    req: Json<RemovePkpFromGroupRequest>,
) -> ApiResponse<AccountOpResponse, ErrMessage> {
    ApiResult(internal::remove_pkp_from_group(req).await).into()
}

#[post("/add_usage_api_key", format = "json", data = "<req>")]
async fn add_usage_api_key(
    req: Json<AddUsageApiKeyRequest>,
) -> ApiResponse<AccountOpResponse, ErrMessage> {
    ApiResult(internal::add_usage_api_key(req).await).into()
}

#[post("/remove_usage_api_key", format = "json", data = "<req>")]
async fn remove_usage_api_key(
    req: Json<RemoveUsageApiKeyRequest>,
) -> ApiResponse<AccountOpResponse, ErrMessage> {
    ApiResult(internal::remove_usage_api_key(req).await).into()
}

#[get("/list_groups?<api_key>&<page_number>&<page_size>")]
async fn list_groups(
    api_key: String,
    page_number: String,
    page_size: String,
) -> ApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    ApiResult(
        internal::list_groups(api_key.as_str(), page_number.as_str(), page_size.as_str()).await,
    )
    .into()
}

#[get("/list_wallets?<api_key>&<page_number>&<page_size>")]
async fn list_wallets(
    api_key: String,
    page_number: String,
    page_size: String,
) -> ApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    ApiResult(
        internal::list_wallets(api_key.as_str(), page_number.as_str(), page_size.as_str()).await,
    )
    .into()
}

#[get("/list_wallets_in_group?<api_key>&<group_id>&<page_number>&<page_size>")]
async fn list_wallets_in_group(
    api_key: String,
    group_id: String,
    page_number: String,
    page_size: String,
) -> ApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    ApiResult(
        internal::list_wallets_in_group(
            api_key.as_str(),
            group_id.as_str(),
            page_number.as_str(),
            page_size.as_str(),
        )
        .await,
    )
    .into()
}

#[get("/list_actions?<api_key>&<group_id>&<page_number>&<page_size>")]
async fn list_actions(
    api_key: String,
    group_id: String,
    page_number: String,
    page_size: String,
) -> ApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    ApiResult(
        internal::list_actions(
            api_key.as_str(),
            group_id.as_str(),
            page_number.as_str(),
            page_size.as_str(),
        )
        .await,
    )
    .into()
}
