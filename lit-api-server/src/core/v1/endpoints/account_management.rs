use std::sync::Arc;

use crate::accounts::signer_pool::SignerPool;
use crate::core::account_management;
use crate::core::v1::guards::apikey::ApiKey;
use crate::core::v1::guards::billing::BilledManagementApiKey;
use crate::core::v1::helpers::api_status::{ApiResult, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{
    AddActionRequest, AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest,
    AddUsageApiKeyRequest, AddUsageApiKeyWithSignatureRequest, ConvertToChainSecuredAccountRequest,
    CreateWalletWithSignatureRequest, DeleteActionRequest, NewAccountRequest,
    RemoveActionFromGroupRequest, RemoveGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest, UpdateUsageApiKeyRequest,
};
use crate::core::v1::models::response::{
    AccountOpResponse, AddGroupResponse, AddUsageApiKeyResponse,
    AddUsageApiKeyWithSignatureResponse, ApiKeyItem, ChainConfigKeysResponse, CreateWalletResponse,
    CreateWalletWithSignatureResponse, ListMetadataItem, NewAccountResponse,
    NodeChainConfigResponse, WalletItem,
};
use crate::stripe::StripeState;
use rocket::State;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::openapi;

#[openapi(tag = "Account Management")]
#[post("/new_account", format = "json", data = "<new_account_request>")]
pub(super) async fn new_account(
    signer_pool: &State<Arc<SignerPool>>,
    stripe_state: &State<Option<Arc<StripeState>>>,
    new_account_request: Json<NewAccountRequest>,
) -> OpenApiResponse<NewAccountResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::new_account(
                signer_pool.inner().clone(),
                stripe_state.inner().clone(),
                new_account_request,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/convert_to_chain_secured_account", format = "json", data = "<req>")]
pub(super) async fn convert_to_chain_secured_account(
    signer_pool: &State<Arc<SignerPool>>,
    stripe_state: &State<Option<Arc<StripeState>>>,
    api_key: BilledManagementApiKey,
    req: Json<ConvertToChainSecuredAccountRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    let api_key_str = api_key.0.clone();
    let result = account_management::convert_to_chain_secured_account(
        signer_pool.inner().clone(),
        api_key_str.as_str(),
        req,
    )
    .await;

    // Conversion changes the account's admin wallet on-chain. The Stripe
    // wallet_cache maps api_key_hash → wallet_address with a 1-hour TTL, so
    // without invalidation, billing/credit lookups would resolve to the old
    // api_payer-generated wallet for up to an hour after conversion.
    if result.is_ok()
        && let Some(stripe) = stripe_state.as_ref()
    {
        crate::stripe::invalidate_wallet_cache(api_key_str.as_str(), stripe).await;
    }

    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/account_exists")]
pub(super) async fn account_exists(api_key: ApiKey) -> OpenApiResponse<bool, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::account_exists(api_key.0.as_str()).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/get_lit_action_ipfs_id", format = "json", data = "<code>")]
pub(super) async fn get_lit_action_ipfs_id(
    code: Json<String>,
) -> OpenApiResponse<String, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_lit_action_ipfs_id(code.into_inner()).await)
            .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_node_chain_config")]
pub(super) async fn get_node_chain_config() -> OpenApiResponse<NodeChainConfigResponse, ErrMessage>
{
    OpenApiResponse {
        response: ApiResult(account_management::get_chain_info().await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_chain_config_keys")]
pub(super) async fn get_chain_config_keys() -> OpenApiResponse<ChainConfigKeysResponse, ErrMessage>
{
    OpenApiResponse {
        response: ApiResult(Ok(account_management::get_chain_config_keys())).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_api_keys?<page_number>&<page_size>")]
pub(super) async fn list_api_keys(
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
pub(super) async fn list_groups(
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
pub(super) async fn list_wallets(
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
pub(super) async fn list_wallets_in_group(
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
pub(super) async fn list_actions(
    api_key: ApiKey,
    group_id: Option<String>,
    page_number: u64,
    page_size: u64,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_actions(
                api_key.0.as_str(),
                group_id.as_deref(),
                page_number,
                page_size,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/create_wallet")]
pub(super) async fn create_wallet(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
) -> OpenApiResponse<CreateWalletResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::create_wallet(signer_pool.inner().clone(), api_key.0.as_str())
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/create_wallet_with_signature", format = "json", data = "<req>")]
pub(super) async fn create_wallet_with_signature(
    req: Json<CreateWalletWithSignatureRequest>,
) -> OpenApiResponse<CreateWalletWithSignatureResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::create_wallet_with_signature(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_group", format = "json", data = "<req>")]
pub(super) async fn add_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
    req: Json<AddGroupRequest>,
) -> OpenApiResponse<AddGroupResponse, ErrMessage> {
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
pub(super) async fn remove_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn add_action(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
#[post("/delete_action", format = "json", data = "<req>")]
pub(super) async fn delete_action(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
    req: Json<DeleteActionRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::delete_action(signer_pool.inner().clone(), api_key.0.as_str(), req)
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_action_to_group", format = "json", data = "<req>")]
pub(super) async fn add_action_to_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn add_pkp_to_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn remove_pkp_from_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
#[post("/add_usage_api_key_with_signature", format = "json", data = "<req>")]
pub(super) async fn add_usage_api_key_with_signature(
    req: Json<AddUsageApiKeyWithSignatureRequest>,
) -> OpenApiResponse<AddUsageApiKeyWithSignatureResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_usage_api_key_with_signature(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_usage_api_key", format = "json", data = "<req>")]
pub(super) async fn add_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn remove_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<RemoveUsageApiKeyRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    let usage_key = req.usage_api_key.clone();
    let result = account_management::remove_usage_api_key(
        signer_pool.inner().clone(),
        api_key.0.as_str(),
        req,
    )
    .await;

    // Evict the deleted usage key from the billing wallet cache so stale
    // mappings are never served after the key is removed on-chain.
    if result.is_ok()
        && let Some(stripe) = stripe_state.as_ref()
    {
        crate::stripe::invalidate_wallet_cache(&usage_key, stripe).await;
    }

    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_usage_api_key", format = "json", data = "<req>")]
pub(super) async fn update_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn update_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn remove_action_from_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn update_action_metadata(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
pub(super) async fn update_usage_api_key_metadata(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
