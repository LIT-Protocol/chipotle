use std::sync::Arc;

use crate::accounts::signer_pool::SignerPool;
use crate::core::account_management;
use crate::core::v1::guards::apikey::ApiKey;
use crate::core::v1::guards::billing::BilledManagementApiKey;
use crate::core::v1::guards::cpu_overload::CpuAvailable;
use crate::core::v1::guards::rate_limit::NewAccountRateLimit;
use crate::core::v1::helpers::api_status::{ApiResult, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{
    AddActionRequest, AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest,
    AddUsageApiKeyRequest, AddUsageApiKeyWithSignatureRequest, ConvertToChainSecuredAccountRequest,
    CreateWalletWithSignatureRequest, DeleteActionRequest, DeleteWalletRequest, NewAccountRequest,
    RemoveActionFromGroupRequest, RemoveGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest, UpdateUsageApiKeyRequest,
};
use crate::core::v1::models::response::{
    AccountOpResponse, AddGroupResponse, AddUsageApiKeyResponse,
    AddUsageApiKeyWithSignatureResponse, ApiKeyItem, ChainConfigKeysResponse, CreateWalletResponse,
    CreateWalletWithSignatureResponse, ListMetadataItem, NewAccountResponse,
    NodeChainConfigResponse, PrepareWalletResponse, WalletItem,
};
use crate::stripe::StripeState;
use rocket::State;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::openapi;

/// Create a new managed account: derives a fresh wallet, registers it on-chain,
/// and provisions a Stripe customer with starter credits. Returns the account's
/// API key and wallet address.
///
/// No authentication is required (this is how a caller obtains their first API
/// key), but the endpoint is rate limited per client IP and may return 429 Too
/// Many Requests when the node is under load or a single source creates
/// accounts too quickly. Retry those with exponential backoff.
// Unauthenticated + expensive (two operator-funded on-chain txs, a Stripe
// customer, and starter credits per call), so it carries two anonymous-abuse
// guards (CPL-367): CpuAvailable sheds load when the node is CPU-bound, and
// NewAccountRateLimit caps the sustained account-creation rate per client IP.
#[openapi(tag = "Account Management")]
#[post("/new_account", format = "json", data = "<new_account_request>")]
pub(super) async fn new_account(
    _cpu: CpuAvailable,
    _rate_limit: NewAccountRateLimit,
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

/// Mint a new wallet (PKP) for the account.
///
/// Deprecated: minting is a metered write, so it should not live on a GET —
/// link previewers, prefetchers, and retrying proxies replay GETs. Use
/// `POST /create_wallet` instead. This form is kept for backwards
/// compatibility.
#[openapi(tag = "Account Management", deprecated)]
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

/// Mint a new wallet (PKP) for the account.
#[openapi(tag = "Account Management")]
#[post("/create_wallet")]
pub(super) async fn create_wallet_post(
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

/// Return a fresh derived wallet address + derivation path — no signature, no API key.
///
/// The no-signature equivalent of `create_wallet_with_signature`: it collapses the
/// ChainSecured owner ceremony into a single signed bind UserOp. Fetch the address
/// here, then register it on-chain yourself with `registerWalletDerivation`.
///
/// Unauthenticated, so it carries the same `CpuAvailable` load-shedding guard as
/// `lit_action`: each request drives a dstack KDF call, and unlike the
/// `_with_signature` siblings there is no EIP-712 verification in front of it, so
/// the guard bounds how hard an anonymous caller can hammer the KDF path when the
/// box is already saturated.
///
/// NOT IDEMPOTENT: every call returns a brand-new wallet (a fresh random derivation
/// path). Retrying returns a different address, and concurrent callers each get a
/// separate wallet with no server-side dedup. See `docs/management/api_direct.mdx`.
#[openapi(tag = "Account Management")]
#[post("/prepare_wallet")]
pub(super) async fn prepare_wallet(
    _cpu: CpuAvailable,
) -> OpenApiResponse<PrepareWalletResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::prepare_wallet().await).into(),
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

/// Permanently delete a wallet (PKP). HARD DELETE: wipes the on-chain derivation path so
/// the key can never be re-derived and anything secured by it becomes unrecoverable.
/// Requires the master (account) API key — usage API keys are rejected on-chain
/// (`NotMasterAccount`).
#[openapi(tag = "Account Management")]
#[post("/delete_wallet", format = "json", data = "<req>")]
pub(super) async fn delete_wallet(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
    req: Json<DeleteWalletRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::delete_wallet(signer_pool.inner().clone(), api_key.0.as_str(), req)
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
