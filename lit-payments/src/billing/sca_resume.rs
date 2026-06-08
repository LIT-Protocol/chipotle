//! `GET /billing/auto_topup_resume?token=...` and
//! `POST /billing/auto_topup_resume/complete` — the SCA recovery handoff
//! consumed by the dashboard recovery page.
//!
//! Flow (plan §6a):
//!   1. The webhook handler hits `authentication_required` on an
//!      off-session PaymentIntent. It stashes the `pi_id` and a fresh
//!      `recovery_token` on the `auto_topup_config` row and emails the
//!      user a link of the form `…/recover_topup?token={token}`.
//!   2. The dashboard recovery page reads `token` from the query string
//!      and calls `GET /billing/auto_topup_resume?token=...`. That
//!      endpoint **single-use-consumes** the token (Postgres atomic
//!      UPDATE) and returns the PI's Stripe `client_secret`.
//!   3. Stripe.js renders the bank's 3DS challenge inside its iframe;
//!      on success, the dashboard calls `POST
//!      /billing/auto_topup_resume/complete` with the PI id. That
//!      endpoint re-fetches the PI to verify it's `succeeded`, applies
//!      the sync credit, and clears the pending state on the config row.
//!
//! Both endpoints are intentionally unauthenticated by the BillingAuth
//! guard: the `recovery_token` IS the credential (one-time, 32 random
//! bytes). Treating it like a session bearer token keeps the recovery
//! page flow simple — the user does not need to re-sign EIP-712 just to
//! complete a charge they already authorised.

use lit_billing_core::StripeClient;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, get, post};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::auto_topup::db;
use crate::config::Config;
use crate::internal::client as internal_client;

#[derive(Debug, Serialize)]
pub struct ResumeResponse {
    pub payment_intent_id: String,
    pub client_secret: String,
    pub publishable_key: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: &'static str,
    pub message: String,
}

fn err(status: Status, code: &'static str, msg: impl Into<String>) -> (Status, Json<ErrorBody>) {
    (
        status,
        Json(ErrorBody {
            error: code,
            message: msg.into(),
        }),
    )
}

#[get("/billing/auto_topup_resume?<token>")]
pub async fn get_auto_topup_resume(
    token: &str,
    cfg: &State<Config>,
    stripe: &State<StripeClient>,
    pool: &State<PgPool>,
) -> Result<Json<ResumeResponse>, (Status, Json<ErrorBody>)> {
    if token.trim().is_empty() {
        return Err(err(
            Status::NotFound,
            "invalid_token",
            "Recovery link is invalid or expired.",
        ));
    }
    // Codex P2 #3: split lookup from consume. The token used to be
    // burned atomically up front; a transient Stripe failure on the
    // next line would 503 the request and the user's second click would
    // 404 with no recovery path. Now we LOOK UP without consuming, call
    // Stripe, and only invalidate the token AFTER Stripe succeeds.
    //
    // Single-use guarantee still holds for the happy path: a successful
    // GET clears the token; the next GET sees no row. A failed Stripe
    // call leaves the token usable for retry (the bug we're fixing).
    let (customer_id, pi_id) = db::lookup_recovery_token(pool, token)
        .await
        .map_err(|e| {
            tracing::error!("auto_topup_resume GET: db error: {e}");
            err(
                Status::ServiceUnavailable,
                "db_unavailable",
                "Could not look up your recovery; try again.",
            )
        })?
        .ok_or_else(|| {
            err(
                Status::NotFound,
                "invalid_token",
                "Recovery link is invalid or expired.",
            )
        })?;

    let resp = stripe
        .get(&format!("payment_intents/{pi_id}"), &[])
        .await
        .map_err(|e| {
            tracing::error!("auto_topup_resume GET: Stripe error: {e}");
            err(
                Status::ServiceUnavailable,
                "stripe_unavailable",
                "Could not retrieve your pending top-up; try again.",
            )
        })?;
    let client_secret = resp
        .body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            err(
                Status::ServiceUnavailable,
                "stripe_response_malformed",
                "Stripe response missing client_secret.",
            )
        })?
        .to_string();

    // Stripe call succeeded — now safe to burn the token. We log but
    // don't fail the response if the clear errors; the user already has
    // their client_secret. A leftover token is a small replay window
    // (24h max), not a correctness break.
    if let Err(e) = db::clear_recovery_token_for_pi(pool, &customer_id, &pi_id).await {
        tracing::warn!(
            customer_id = %customer_id,
            pi_id = %pi_id,
            "auto_topup_resume GET: clear_recovery_token_for_pi failed: {e}"
        );
    }

    Ok(Json(ResumeResponse {
        payment_intent_id: pi_id,
        client_secret,
        publishable_key: cfg.stripe_publishable_key.clone(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CompleteRequest {
    pub payment_intent_id: String,
}

/// Called by the dashboard after `stripe.confirmCardPayment` returns
/// `succeeded`. Verifies status server-side against Stripe (do not trust
/// the client claim), applies the sync credit, and clears pending state.
///
/// Idempotent: the `auto_topup_credits` UNIQUE constraint guarantees a
/// repeated call doesn't double-credit; a successful call clears the
/// config-row pending fields so subsequent calls see no work.
#[post(
    "/billing/auto_topup_resume/complete",
    format = "json",
    data = "<body>"
)]
pub async fn post_auto_topup_resume_complete(
    body: Json<CompleteRequest>,
    cfg: &State<Config>,
    stripe: &State<StripeClient>,
    pool: &State<PgPool>,
) -> Result<Json<serde_json::Value>, (Status, Json<ErrorBody>)> {
    let pi_id = body.into_inner().payment_intent_id;
    if pi_id.trim().is_empty() {
        return Err(err(
            Status::BadRequest,
            "missing_pi_id",
            "payment_intent_id is required.",
        ));
    }

    let pi = stripe
        .get(&format!("payment_intents/{pi_id}"), &[])
        .await
        .map_err(|e| {
            tracing::error!("resume/complete: Stripe retrieve failed: {e}");
            err(
                Status::ServiceUnavailable,
                "stripe_unavailable",
                "Could not verify your top-up; try again.",
            )
        })?
        .body;

    let status_str = pi.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status_str != "succeeded" {
        return Err(err(
            Status::BadRequest,
            "pi_not_succeeded",
            format!(
                "Your top-up has not completed yet (status: {status_str}). Finish the bank verification first."
            ),
        ));
    }
    // Defence in depth: only credit PIs we minted.
    if pi.pointer("/metadata/source").and_then(|v| v.as_str()) != Some("auto_topup") {
        return Err(err(
            Status::BadRequest,
            "not_auto_topup_pi",
            "That PaymentIntent is not an auto top-up.",
        ));
    }

    let customer_id = pi
        .get("customer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            err(
                Status::BadRequest,
                "pi_missing_customer",
                "PaymentIntent is missing a customer reference.",
            )
        })?
        .to_string();
    let amount = pi.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
    if amount <= 0 {
        return Err(err(
            Status::BadRequest,
            "pi_amount_invalid",
            "PaymentIntent amount is invalid.",
        ));
    }

    let claimed = db::try_insert_credit(pool, &pi_id, &customer_id, amount)
        .await
        .map_err(|e| {
            tracing::error!("resume/complete: try_insert_credit failed: {e}");
            err(
                Status::ServiceUnavailable,
                "db_unavailable",
                "Could not record your credit; try again.",
            )
        })?;
    if claimed {
        let credit_idem = format!("credit:{pi_id}");
        let neg = (-amount).to_string();
        let bt_resp = stripe
            .post_with_idempotency(
                &format!("customers/{customer_id}/balance_transactions"),
                &[
                    ("amount", neg.as_str()),
                    ("currency", "usd"),
                    ("description", &format!("Auto top-up via {pi_id}")),
                ],
                &credit_idem,
            )
            .await
            .map_err(|e| {
                tracing::error!("resume/complete: balance_tx failed: {e}");
                err(
                    Status::ServiceUnavailable,
                    "stripe_unavailable",
                    "Could not credit your account; try again.",
                )
            })?;
        let bt_id = bt_resp
            .body
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                err(
                    Status::ServiceUnavailable,
                    "stripe_response_malformed",
                    "Stripe response missing balance_transaction id.",
                )
            })?
            .to_string();
        let _ = db::mark_credit_completed(pool, &pi_id, &bt_id).await;
    }
    let _ = db::clear_pending_action(pool, &customer_id, &pi_id).await;
    let _ = invalidate_cache(cfg, &customer_id).await;

    Ok(Json(serde_json::json!({
        "credited": claimed,
        "payment_intent_id": pi_id,
    })))
}

async fn invalidate_cache(cfg: &Config, customer_id: &str) -> anyhow::Result<()> {
    let client = internal_client::build_client()?;
    let body = serde_json::json!({ "customer_id": customer_id });
    internal_client::post_internal(&client, cfg, "/internal/invalidate_balance_cache", &body).await
}
