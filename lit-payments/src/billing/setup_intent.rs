//! `POST /billing/setup_intent` — save-card flow entry point.
//!
//! Returns a Stripe SetupIntent `client_secret` plus the publishable key so
//! the dashboard can initialise Stripe.js in setup mode and call
//! `stripe.confirmCardSetup`. The card is attached to the caller's existing
//! Stripe customer; SCA prior-auth (3DS challenge during save) happens
//! client-side for EU cards.
//!
//! ### Customer bootstrap requirement
//!
//! We deliberately do NOT call `find_or_create_by_wallet` here. If a wallet
//! has never made a manual top-up, no Stripe customer exists, and we return
//! 400 with a clear "make a manual top-up first" message. Rationale: the
//! manual top-up flow is the canonical Customer-creation path and goes
//! through KYC-relevant code; auto-creating customers on a SetupIntent
//! request would let any auth'd wallet spawn empty Stripe customers.

use lit_billing_auth::BillingAuth;
use lit_billing_core::{StripeClient, customer};
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, post};
use serde::Serialize;
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Serialize)]
pub struct SetupIntentResponse {
    /// Pass to `stripe.confirmCardSetup(client_secret)` in the dashboard.
    pub client_secret: String,
    /// Initialise Stripe.js with this. Same value across all users on a
    /// given Stripe account; safe to ship to the browser.
    pub publishable_key: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: &'static str,
    pub message: String,
}

#[post("/billing/setup_intent")]
pub async fn setup_intent(
    auth: BillingAuth,
    cfg: &State<Config>,
    stripe: &State<StripeClient>,
) -> Result<Json<SetupIntentResponse>, (Status, Json<ErrorBody>)> {
    // Resolve the auth'd identity to the wallet address Stripe customers are
    // keyed by. For WalletSigned this is in-band; for ApiKey we'd need an
    // on-chain hop. Phase 3 ships only the wallet-sig path — API-key
    // callers get a clear-message rejection until the resolver hop is
    // wired in Phase 4 (config CRUD) or later.
    let wallet_address = match &auth {
        BillingAuth::WalletSigned {
            wallet_address_hex, ..
        } => wallet_address_hex.clone(),
        BillingAuth::ApiKey(_) => {
            return Err((
                Status::NotImplemented,
                Json(ErrorBody {
                    error: "api_key_setup_intent_unsupported",
                    message:
                        "API-key callers must use the dashboard wallet-sign flow to save a card."
                            .to_string(),
                }),
            ));
        }
    };

    let customer_id = match customer::find_by_wallet(stripe.inner(), &wallet_address).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Err((
                Status::BadRequest,
                Json(ErrorBody {
                    error: "no_stripe_customer",
                    message:
                        "Make a one-time top-up first to set up billing, then come back to save a card."
                            .to_string(),
                }),
            ));
        }
        Err(e) => {
            tracing::error!("setup_intent: customer lookup failed: {e}");
            return Err((
                Status::ServiceUnavailable,
                Json(ErrorBody {
                    error: "stripe_lookup_failed",
                    message: "Could not contact Stripe to look up your customer record; try again."
                        .to_string(),
                }),
            ));
        }
    };

    // Random UUID is fine — there is nothing to dedupe against. A retried
    // setup_intent for the same user is harmless (Stripe returns a fresh
    // intent each time; the dashboard only confirms one).
    let idempotency_key = Uuid::new_v4().to_string();
    let resp = stripe
        .post_with_idempotency(
            "setup_intents",
            &[("usage", "off_session"), ("customer", customer_id.as_str())],
            &idempotency_key,
        )
        .await
        .map_err(|e| {
            tracing::error!("setup_intent: SetupIntent create failed: {e}");
            (
                Status::ServiceUnavailable,
                Json(ErrorBody {
                    error: "stripe_setup_intent_failed",
                    message: "Stripe rejected the SetupIntent create; try again.".to_string(),
                }),
            )
        })?;

    let client_secret = resp
        .body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            tracing::error!("setup_intent: Stripe response missing client_secret");
            (
                Status::ServiceUnavailable,
                Json(ErrorBody {
                    error: "stripe_response_malformed",
                    message: "Stripe response missing client_secret.".to_string(),
                }),
            )
        })?
        .to_string();

    Ok(Json(SetupIntentResponse {
        client_secret,
        publishable_key: cfg.stripe_publishable_key.clone(),
    }))
}
