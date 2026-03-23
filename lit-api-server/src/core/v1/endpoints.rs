use std::sync::Arc;

use crate::accounts::signer_pool::SignerPool;
use crate::actions::grpc::GrpcClientPool;
use crate::core::account_management;
use crate::core::core_features;
use crate::core::v1::guards::apikey::ApiKey;
use crate::core::v1::guards::billing::{BilledLitActionApiKey, BilledManagementApiKey};
use crate::core::v1::guards::cpu_overload::CpuAvailable;
use crate::core::v1::helpers::api_status::{ApiResult, ApiStatus, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{
    AddActionRequest, AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest,
    AddUsageApiKeyRequest, ConfirmPaymentRequest, CreatePaymentIntentRequest, LitActionRequest,
    NewAccountRequest, RemoveActionFromGroupRequest, RemoveGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest, UpdateUsageApiKeyRequest,
};
use crate::core::v1::models::response::ApiKeyItem;
use crate::core::v1::models::response::WalletItem;
use crate::core::v1::models::response::{
    AccountOpResponse, AddUsageApiKeyResponse, CreateWalletResponse, ListMetadataItem,
    LitActionResponse, NewAccountResponse, NodeChainConfigResponse, VersionResponse,
    AccountOpResponse, AddUsageApiKeyResponse, BillingBalanceResponse, CreatePaymentIntentResponse,
    CreateWalletResponse, ListMetadataItem, LitActionResponse, NewAccountResponse,
    NodeChainConfigResponse, StripeConfigResponse,
};
use crate::observability::RequestSpan;
use crate::stripe::{self, StripeState};
use moka::future::Cache;
use rocket::Route;
use rocket::State;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::okapi::openapi3::OpenApi;
use rocket_okapi::openapi;
use rocket_okapi::openapi_get_routes_spec;

/// Returns Core v1 routes and the OpenAPI spec for them.
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
        get_version,
        billing_stripe_config,
        billing_balance,
        billing_create_payment_intent,
        billing_confirm_payment,
    ]
}

// ─── Account creation (not behind billing — creating an account is free) ──────

#[openapi(tag = "Account Management")]
#[post("/new_account", format = "json", data = "<new_account_request>")]
async fn new_account(
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

// ─── Read-only endpoints (no billing) ─────────────────────────────────────────

#[openapi(tag = "Account Management")]
#[get("/account_exists")]
async fn account_exists(api_key: ApiKey) -> OpenApiResponse<bool, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::account_exists(api_key.0.as_str()).await).into(),
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

// ─── List endpoints (no billing charge) ───────────────────────────────────────

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

// ─── Billed management endpoints ($0.01 each) ─────────────────────────────────

#[openapi(tag = "Account Management")]
#[get("/create_wallet")]
async fn create_wallet(
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
#[post("/add_group", format = "json", data = "<req>")]
async fn add_group(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
async fn add_action(
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
#[post("/add_action_to_group", format = "json", data = "<req>")]
async fn add_action_to_group(
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
async fn add_pkp_to_group(
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
async fn remove_pkp_from_group(
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
#[post("/add_usage_api_key", format = "json", data = "<req>")]
async fn add_usage_api_key(
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
async fn remove_usage_api_key(
    signer_pool: &State<Arc<SignerPool>>,
    api_key: BilledManagementApiKey,
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
async fn update_group(
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
async fn remove_action_from_group(
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
async fn update_action_metadata(
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
async fn update_usage_api_key_metadata(
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

// ─── Billed Lit Action endpoint ($0.01) ───────────────────────────────────────

#[openapi(tag = "Actions")]
#[post("/lit_action", format = "json", data = "<lit_action_request>")]
#[tracing::instrument(name = "endpoint::lit_action", skip_all, parent = &request_span.span)]
async fn lit_action(
    _cpu: CpuAvailable,
    request_span: RequestSpan,
    api_key: BilledLitActionApiKey,
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

// ─── Billing endpoints ─────────────────────────────────────────────────────────

fn billing_disabled_err() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Stripe billing is not configured on this node"),
        "Billing not configured",
    )
}

/// GET /billing/stripe_config — returns the Stripe publishable key.
/// No auth required; the publishable key is safe to expose.
#[openapi(tag = "Billing")]
#[get("/billing/stripe_config")]
async fn billing_stripe_config(
    stripe_state: &State<Option<Arc<StripeState>>>,
) -> OpenApiResponse<StripeConfigResponse, ErrMessage> {
    let result = match stripe_state.inner() {
        Some(s) => Ok(StripeConfigResponse {
            publishable_key: s.publishable_key.clone(),
        }),
        None => Err(billing_disabled_err()),
    };
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

/// GET /billing/balance — returns the current credit balance for the authenticated user.
#[openapi(tag = "Billing")]
#[get("/billing/balance")]
async fn billing_balance(
    api_key: ApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
) -> OpenApiResponse<BillingBalanceResponse, ErrMessage> {
    let result = billing_balance_impl(api_key.0.as_str(), stripe_state.inner()).await;
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

async fn billing_balance_impl(
    api_key: &str,
    stripe_state: &Option<Arc<StripeState>>,
) -> Result<BillingBalanceResponse, ApiStatus> {
    let stripe = stripe_state.as_ref().ok_or_else(billing_disabled_err)?;
    let wallet = stripe::api_key_to_wallet_address(api_key)
        .map_err(|e| ApiStatus::bad_request(e, "Invalid API key"))?;
    // Look up an existing Stripe customer without creating one.
    let customer_id = stripe::get_customer_by_wallet(&wallet, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    let balance = stripe::get_credit_balance(&customer_id, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    let credits = -balance; // positive = available credit cents
    let display = if credits <= 0 {
        "No credits".to_string()
    } else {
        format!("{} credit", stripe::cents_to_display(credits))
    };
    Ok(BillingBalanceResponse {
        balance_cents: balance,
        balance_display: display,
    })
}

/// POST /billing/create_payment_intent — creates a Stripe PaymentIntent and returns
/// the client_secret for use with Stripe.js `confirmCardPayment`.
#[openapi(tag = "Billing")]
#[post("/billing/create_payment_intent", format = "json", data = "<req>")]
async fn billing_create_payment_intent(
    api_key: ApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<CreatePaymentIntentRequest>,
) -> OpenApiResponse<CreatePaymentIntentResponse, ErrMessage> {
    let result =
        billing_create_payment_intent_impl(api_key.0.as_str(), stripe_state.inner(), req).await;
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

async fn billing_create_payment_intent_impl(
    api_key: &str,
    stripe_state: &Option<Arc<StripeState>>,
    req: Json<CreatePaymentIntentRequest>,
) -> Result<CreatePaymentIntentResponse, ApiStatus> {
    let stripe = stripe_state.as_ref().ok_or_else(billing_disabled_err)?;
    let wallet = stripe::api_key_to_wallet_address(api_key)
        .map_err(|e| ApiStatus::bad_request(e, "Invalid API key"))?;
    let (client_secret, payment_intent_id) =
        stripe::create_payment_intent(&wallet, req.amount_cents, stripe)
            .await
            .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    Ok(CreatePaymentIntentResponse {
        client_secret,
        payment_intent_id,
    })
}

/// POST /billing/confirm_payment — verifies a succeeded PaymentIntent and credits the account.
#[openapi(tag = "Billing")]
#[post("/billing/confirm_payment", format = "json", data = "<req>")]
async fn billing_confirm_payment(
    api_key: ApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<ConfirmPaymentRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    let result = billing_confirm_payment_impl(api_key.0.as_str(), stripe_state.inner(), req).await;
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

async fn billing_confirm_payment_impl(
    api_key: &str,
    stripe_state: &Option<Arc<StripeState>>,
    req: Json<ConfirmPaymentRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let stripe = stripe_state.as_ref().ok_or_else(billing_disabled_err)?;
    let wallet = stripe::api_key_to_wallet_address(api_key)
        .map_err(|e| ApiStatus::bad_request(e, "Invalid API key"))?;
    stripe::confirm_payment_and_credit(&req.payment_intent_id, &wallet, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    Ok(AccountOpResponse { success: true })
}

#[openapi(tag = "Info")]
#[get("/version")]
async fn get_version() -> OpenApiResponse<VersionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(Ok(VersionResponse {
            version: crate::version::VERSION,
            src_hash: crate::version::SRC_HASH,
            git_commit: crate::version::GIT_COMMIT,
        }))
        .into(),
    }
}
