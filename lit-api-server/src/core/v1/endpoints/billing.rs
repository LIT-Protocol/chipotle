use std::sync::Arc;

use crate::core::v1::helpers::api_status::{ApiResult, ApiStatus, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{ConfirmPaymentRequest, CreatePaymentIntentRequest};
use crate::core::v1::models::response::{
    AccountOpResponse, BillingBalanceResponse, CreatePaymentIntentResponse, StripeConfigResponse,
};
use crate::stripe::{self, StripeState, WebhookHandleError};
use lit_billing_core::billing_auth::BillingAuth;
use rocket::data::ToByteUnit;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use rocket::{Data, Route, State, get, post, routes};
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
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

/// GET /billing/balance — returns the current credit balance for the authenticated user.
#[openapi(tag = "Billing")]
#[get("/billing/balance")]
pub(super) async fn billing_balance(
    auth: BillingAuth,
    stripe_state: &State<Option<Arc<StripeState>>>,
) -> OpenApiResponse<BillingBalanceResponse, ErrMessage> {
    let result = billing_balance_impl(auth.identity_string(), stripe_state.inner()).await;
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
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
    auth: BillingAuth,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<CreatePaymentIntentRequest>,
) -> OpenApiResponse<CreatePaymentIntentResponse, ErrMessage> {
    let result =
        billing_create_payment_intent_impl(auth.identity_string(), stripe_state.inner(), req).await;
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
    auth: BillingAuth,
    stripe_state: &State<Option<Arc<StripeState>>>,
    req: Json<ConfirmPaymentRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    let result =
        billing_confirm_payment_impl(auth.identity_string(), stripe_state.inner(), req).await;
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
    let wallet = stripe::resolve_wallet_address(api_key, stripe)
        .await
        .map_err(wallet_resolution_err)?;
    stripe::confirm_payment_and_credit(&req.payment_intent_id, &wallet, stripe)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Stripe error"))?;
    Ok(AccountOpResponse { success: true })
}

/// Maximum accepted webhook body. Stripe events are small (a few KB); a 256 KiB
/// ceiling is generous while bounding memory from a hostile uncapped POST.
const MAX_WEBHOOK_BODY: u64 = 256 * 1024;

/// Extracts the `Stripe-Signature` header so the handler can verify the body.
/// A delivery without it is rejected before we read the body.
struct StripeSignature(String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for StripeSignature {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("Stripe-Signature") {
            Some(sig) => Outcome::Success(StripeSignature(sig.to_string())),
            None => Outcome::Error((Status::BadRequest, ())),
        }
    }
}

/// POST /billing/webhook — Stripe-signed event ingress (CPL-335).
///
/// Verifies the `Stripe-Signature` over the *raw* body (so it must read bytes,
/// not deserialized JSON), then applies refund/dispute clawbacks to the credit
/// ledger. Mounted outside the OpenAPI surface (like `/health`) — it's a
/// machine-to-machine endpoint, not part of the documented client API.
///
/// Returns 2xx on success or duplicate (Stripe stops retrying); 5xx on a
/// transient failure (Stripe retries); 4xx on a bad signature / payload.
#[post("/billing/webhook", data = "<body>")]
async fn billing_webhook(
    sig: StripeSignature,
    stripe_state: &State<Option<Arc<StripeState>>>,
    body: Data<'_>,
) -> Status {
    let Some(stripe) = stripe_state.inner() else {
        // Billing disabled entirely on this node.
        return Status::ServiceUnavailable;
    };

    let bytes = match body.open(MAX_WEBHOOK_BODY.bytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        Ok(_) => {
            tracing::warn!("stripe webhook: body exceeded {MAX_WEBHOOK_BODY} bytes");
            return Status::PayloadTooLarge;
        }
        Err(e) => {
            tracing::warn!("stripe webhook: failed to read body: {e}");
            return Status::BadRequest;
        }
    };

    match stripe::handle_webhook(&bytes, &sig.0, stripe).await {
        Ok(()) => Status::Ok,
        Err(WebhookHandleError::NotConfigured) => {
            tracing::warn!("stripe webhook: received but STRIPE_WEBHOOK_SECRET not configured");
            Status::ServiceUnavailable
        }
        Err(e @ WebhookHandleError::InvalidSignature(_))
        | Err(e @ WebhookHandleError::BadPayload(_)) => {
            tracing::warn!("stripe webhook: rejected: {e}");
            Status::BadRequest
        }
        Err(e @ WebhookHandleError::Transient(_)) => {
            // 5xx so Stripe retries — the clawback hasn't been applied yet.
            tracing::error!("stripe webhook: transient failure, asking Stripe to retry: {e}");
            Status::InternalServerError
        }
    }
}

/// Webhook route, mounted separately from the OpenAPI-documented routes (see
/// `main::build_rocket`). Kept out of `routes_with_spec` so it stays absent
/// from `openapi.json` and the generated k6 client.
pub fn webhook_routes() -> Vec<Route> {
    routes![billing_webhook]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Header};
    use rocket::local::asynchronous::Client;

    fn build_rocket(stripe_state: Option<Arc<StripeState>>) -> rocket::Rocket<rocket::Build> {
        rocket::build()
            .manage(stripe_state)
            .mount("/", webhook_routes())
    }

    #[tokio::test]
    async fn webhook_without_signature_header_is_rejected() {
        // The Stripe-Signature guard fails before we even read the body.
        let client = Client::tracked(build_rocket(None))
            .await
            .expect("valid rocket");
        let response = client
            .post("/billing/webhook")
            .header(ContentType::JSON)
            .body(r#"{"id":"evt_1","type":"charge.refunded"}"#)
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::BadRequest);
    }

    #[tokio::test]
    async fn webhook_returns_503_when_billing_disabled() {
        // Signature header present, but no Stripe state configured on this node.
        let client = Client::tracked(build_rocket(None))
            .await
            .expect("valid rocket");
        let response = client
            .post("/billing/webhook")
            .header(ContentType::JSON)
            .header(Header::new("Stripe-Signature", "t=1,v1=deadbeef"))
            .body(r#"{"id":"evt_1","type":"charge.refunded"}"#)
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::ServiceUnavailable);
    }

    /// A missing account must map to 400, not 500. The wallet-lookup helpers
    /// run the contract revert through `decode_contract_revert`, so the error
    /// string contains the `AccountDoesNotExist` error name — this asserts the
    /// substring match still routes it to a client error.
    #[test]
    fn account_does_not_exist_maps_to_400() {
        let err = anyhow::anyhow!("Contract error: AccountDoesNotExist (0xd4a84737...)");
        assert_eq!(wallet_resolution_err(err).status, Status::BadRequest);
    }

    #[test]
    fn missing_wallet_address_maps_to_400() {
        let err = anyhow::anyhow!("account has no wallet address");
        assert_eq!(wallet_resolution_err(err).status, Status::BadRequest);
    }

    /// RPC/transport failures (anything that isn't a known missing-account
    /// revert) stay 500 so transient infra problems aren't reported to clients
    /// as a bad API key.
    #[test]
    fn other_errors_map_to_500() {
        let err = anyhow::anyhow!("error sending request: connection refused");
        assert_eq!(
            wallet_resolution_err(err).status,
            Status::InternalServerError
        );
    }
}
