use std::sync::Arc;

use crate::core::v1::guards::apikey::ApiKey;
use crate::core::v1::helpers::api_status::{ApiResult, ApiStatus, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{ConfirmPaymentRequest, CreatePaymentIntentRequest};
use crate::core::v1::models::response::{
    AccountOpResponse, BillingBalanceResponse, CreatePaymentIntentResponse, StripeConfigResponse,
};
use crate::stripe::{self, StripeState};
use rocket::State;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::openapi;

pub(super) fn billing_disabled_err() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Stripe billing is not configured on this node"),
        "Billing not configured",
    )
}

/// Map wallet resolution errors to the correct HTTP status.
///
/// "account has no wallet address" and contract reverts for missing accounts → 400.
/// Everything else (RPC failures, timeouts) → 500.
fn wallet_resolution_err(e: anyhow::Error) -> ApiStatus {
    let msg = e.to_string();
    if msg.contains("account has no wallet address") || msg.contains("AccountDoesNotExist") {
        ApiStatus::bad_request(
            anyhow::anyhow!("account not found for API key"),
            "Invalid API key",
        )
    } else {
        // Log the underlying error for internal diagnostics without exposing details to clients.
        eprintln!("wallet_resolution_err internal failure: {e:?}");
        ApiStatus::internal_server_error(
            anyhow::anyhow!("internal billing lookup error"),
            "Billing lookup failed",
        )
    }
}

/// GET /billing/stripe_config — returns the Stripe publishable key.
/// No auth required; the publishable key is safe to expose.
#[openapi(tag = "Billing")]
#[get("/billing/stripe_config")]
pub(super) async fn billing_stripe_config(
    stripe_state: &State<Option<Arc<StripeState>>>,
) -> OpenApiResponse<StripeConfigResponse, ErrMessage> {
    let result = match stripe_state.inner() {
        Some(s) => Ok(StripeConfigResponse {
            publishable_key: s.publishable_key.clone(),
        }),
        None => Err(billing_disabled_err()),
    };
    OpenApiResponse::new(ApiResult(result).into())
}

/// GET /billing/balance — returns the current credit balance for the authenticated user.
#[openapi(tag = "Billing")]
#[get("/billing/balance")]
pub(super) async fn billing_balance(
    api_key: ApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
) -> OpenApiResponse<BillingBalanceResponse, ErrMessage> {
    let result = billing_balance_impl(api_key.0.as_str(), stripe_state.inner()).await;
    OpenApiResponse::new(ApiResult(result).into())
}

async fn billing_balance_impl(
    api_key: &str,
    stripe_state: &Option<Arc<StripeState>>,
) -> Result<BillingBalanceResponse, ApiStatus> {
    let stripe = stripe_state.as_ref().ok_or_else(billing_disabled_err)?;
    let wallet = stripe::resolve_wallet_address(api_key, stripe)
        .await
        .map_err(wallet_resolution_err)?;
    let customer_id = stripe::get_customer_by_wallet(&wallet, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    let balance = stripe::get_credit_balance(&customer_id, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    let credits = -balance;
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
pub(super) async fn billing_create_payment_intent(
    api_key: ApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<CreatePaymentIntentRequest>,
) -> OpenApiResponse<CreatePaymentIntentResponse, ErrMessage> {
    let result =
        billing_create_payment_intent_impl(api_key.0.as_str(), stripe_state.inner(), req).await;
    OpenApiResponse::new(ApiResult(result).into())
}

async fn billing_create_payment_intent_impl(
    api_key: &str,
    stripe_state: &Option<Arc<StripeState>>,
    req: Json<CreatePaymentIntentRequest>,
) -> Result<CreatePaymentIntentResponse, ApiStatus> {
    let stripe = stripe_state.as_ref().ok_or_else(billing_disabled_err)?;
    let wallet = stripe::resolve_wallet_address(api_key, stripe)
        .await
        .map_err(wallet_resolution_err)?;
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
pub(super) async fn billing_confirm_payment(
    api_key: ApiKey,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<ConfirmPaymentRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    let result = billing_confirm_payment_impl(api_key.0.as_str(), stripe_state.inner(), req).await;
    OpenApiResponse::new(ApiResult(result).into())
}

async fn billing_confirm_payment_impl(
    api_key: &str,
    stripe_state: &Option<Arc<StripeState>>,
    req: Json<ConfirmPaymentRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let stripe = stripe_state.as_ref().ok_or_else(billing_disabled_err)?;
    let wallet = stripe::resolve_wallet_address(api_key, stripe)
        .await
        .map_err(wallet_resolution_err)?;
    stripe::confirm_payment_and_credit(&req.payment_intent_id, &wallet, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    Ok(AccountOpResponse { success: true })
}
