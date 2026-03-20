use std::sync::Arc;

use crate::accounts::signer_pool::SignerPool;
use crate::actions::grpc::GrpcClientPool;
use crate::core::account_management;
use crate::core::core_features;
use crate::core::v1::guards::apikey::ApiKey;
use crate::core::v1::guards::cpu_overload::CpuAvailable;
use crate::core::v1::helpers::api_status::{ApiResult, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{
    AddActionRequest, AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest,
    AddUsageApiKeyRequest, LitActionRequest, NewAccountRequest, RemoveActionFromGroupRequest,
    RemoveGroupRequest, RemovePkpFromGroupRequest, RemoveUsageApiKeyRequest,
    UpdateActionMetadataRequest, UpdateGroupRequest, UpdateUsageApiKeyMetadataRequest,
    UpdateUsageApiKeyRequest,
};
use crate::core::v1::models::response::ApiKeyItem;
use crate::core::v1::models::response::WalletItem;
use crate::core::v1::models::response::{
    AccountOpResponse, AddUsageApiKeyResponse, CreateWalletResponse, ListMetadataItem,
    LitActionResponse, NewAccountResponse, NodeChainConfigResponse,
};
use crate::observability::RequestSpan;
use moka::future::Cache;
use rocket::Route;
use rocket::State;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::okapi::openapi3::OpenApi;
use rocket_okapi::openapi;
use rocket_okapi::openapi_get_routes_spec;

/// Returns Core v1 routes and the OpenAPI spec for them. Mount routes at `/core/v1/` and serve the spec at `/openapi.json` (or use it for Swagger UI).
pub fn routes_with_spec() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        list_api_keys,
        new_account,
        account_exists,
        create_wallet,
        lit_action,
        get_lit_action_ipfs_id,
        add_group,
        remove_group,
        add_action,
        add_action_to_group,
        add_pkp_to_group,
        remove_pkp_from_group,
        add_usage_api_key,
        update_usage_api_key,
        remove_usage_api_key,
        update_group,
        remove_action_from_group,
        update_action_metadata,
        update_usage_api_key_metadata,
        list_groups,
        list_wallets,
        list_wallets_in_group,
        list_actions,
        get_node_chain_config,
        get_api_payers,
        get_admin_api_payer,
    ]
}

#[openapi(tag = "Account Management")]
#[post("/new_account", format = "json", data = "<new_account_request>")]
async fn new_account(
    signer_pool: &State<Arc<SignerPool>>,
    new_account_request: Json<NewAccountRequest>,
) -> OpenApiResponse<NewAccountResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::new_account(signer_pool.inner().clone(), new_account_request).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/account_exists")]
async fn account_exists(api_key: ApiKey) -> OpenApiResponse<bool, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::account_exists(api_key.0.as_str()).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/create_wallet")]
async fn create_wallet(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
) -> OpenApiResponse<CreateWalletResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::create_wallet(signer_pool.inner().clone(), api_key.0.as_str())
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Actions")]
#[post("/lit_action", format = "json", data = "<lit_action_request>")]
#[tracing::instrument(name = "endpoint::lit_action", skip_all, parent = &request_span.span)]
async fn lit_action(
    _cpu: CpuAvailable,
    request_span: RequestSpan,
    api_key: ApiKey,
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, String>>,
    http_client: &State<reqwest::Client>,
    lit_action_request: Json<LitActionRequest>,
) -> OpenApiResponse<LitActionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::lit_action(
                &request_span,
                api_key.0.as_str(),
                grpc_client_pool.inner(),
                ipfs_cache.inner(),
                http_client.inner(),
                lit_action_request,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/get_lit_action_ipfs_id", format = "json", data = "<code>")]
async fn get_lit_action_ipfs_id(code: Json<String>) -> OpenApiResponse<String, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_lit_action_ipfs_id(code.into_inner()).await)
            .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_group", format = "json", data = "<req>")]
async fn add_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<AddGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::add_group(signer_pool.inner().clone(), api_key.0.as_str(), req)
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_group", format = "json", data = "<req>")]
async fn remove_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<RemoveGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_group(signer_pool.inner().clone(), api_key.0.as_str(), req)
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_action", format = "json", data = "<req>")]
async fn add_action(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<AddActionRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::add_action(signer_pool.inner().clone(), api_key.0.as_str(), req)
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_action_to_group", format = "json", data = "<req>")]
async fn add_action_to_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<AddActionToGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::add_action_to_group(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_pkp_to_group", format = "json", data = "<req>")]
async fn add_pkp_to_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<AddPkpToGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::add_pkp_to_group(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_pkp_from_group", format = "json", data = "<req>")]
async fn remove_pkp_from_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<RemovePkpFromGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_pkp_from_group(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_usage_api_key", format = "json", data = "<req>")]
async fn add_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<AddUsageApiKeyRequest>,
) -> OpenApiResponse<AddUsageApiKeyResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::add_usage_api_key(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_usage_api_key", format = "json", data = "<req>")]
async fn remove_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<RemoveUsageApiKeyRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_usage_api_key(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_usage_api_key", format = "json", data = "<req>")]
async fn update_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<UpdateUsageApiKeyRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::update_usage_api_key(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_group", format = "json", data = "<req>")]
async fn update_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<UpdateGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::update_group(signer_pool.inner().clone(), api_key.0.as_str(), req)
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_action_from_group", format = "json", data = "<req>")]
async fn remove_action_from_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<RemoveActionFromGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_action_from_group(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_action_metadata", format = "json", data = "<req>")]
async fn update_action_metadata(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<UpdateActionMetadataRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::update_action_metadata(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_usage_api_key_metadata", format = "json", data = "<req>")]
async fn update_usage_api_key_metadata(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: ApiKey,
    req: Json<UpdateUsageApiKeyMetadataRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::update_usage_api_key_metadata(
                signer_pool.inner().clone(),
                api_key.0.as_str(),
                req,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_api_keys?<page_number>&<page_size>")]
async fn list_api_keys(
    api_key: ApiKey,
    page_number: u64,
    page_size: u64,
) -> OpenApiResponse<Vec<ApiKeyItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_api_keys(api_key.0.as_str(), page_number, page_size).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_groups?<page_number>&<page_size>")]
async fn list_groups(
    api_key: ApiKey,
    page_number: u64,
    page_size: u64,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_groups(api_key.0.as_str(), page_number, page_size).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_wallets?<page_number>&<page_size>")]
async fn list_wallets(
    api_key: ApiKey,
    page_number: u64,
    page_size: u64,
) -> OpenApiResponse<Vec<WalletItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_wallets(api_key.0.as_str(), page_number, page_size).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_wallets_in_group?<group_id>&<page_number>&<page_size>")]
async fn list_wallets_in_group(
    api_key: ApiKey,
    group_id: u64,
    page_number: u64,
    page_size: u64,
) -> OpenApiResponse<Vec<WalletItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_wallets_in_group(
                api_key.0.as_str(),
                group_id,
                page_number,
                page_size,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_actions?<group_id>&<page_number>&<page_size>")]
async fn list_actions(
    api_key: ApiKey,
    group_id: String,
    page_number: u64,
    page_size: u64,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_actions(
                api_key.0.as_str(),
                group_id.as_str(),
                page_number,
                page_size,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_node_chain_config")]
async fn get_node_chain_config() -> OpenApiResponse<NodeChainConfigResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_chain_info().await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_api_payers")]
async fn get_api_payers() -> OpenApiResponse<Vec<String>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_api_payers().await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_admin_api_payer")]
async fn get_admin_api_payer() -> OpenApiResponse<String, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_admin_api_payer().await).into(),
    }
}
