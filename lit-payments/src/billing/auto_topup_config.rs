//! `GET` / `PUT /billing/auto_topup_config` — dashboard CRUD for the
//! per-user auto top-up rule.
//!
//! Behind the [`BillingAuth`] guard. Wallet-sig path is fully supported;
//! API-key callers get 501 until the resolver hop is wired (Phase 5 needs
//! the on-chain mapping anyway).
//!
//! The PUT path enforces:
//!   - `cap >= topup_amount` (cap must cover at least one top-up)
//!   - `topup_amount >= $5` (matches the existing manual-topup floor)
//!   - `payment_method_id` belongs to the caller's Stripe customer
//!     (prevents wallet A from setting a `pm_xxx` that belongs to wallet B)
//!
//! On the disable transition (`enabled = false`), the DB layer NULLs out
//! pending SCA-recovery state — closing codex's gap #15 about stale
//! pending_action_pi_id triggering charges after a user opts out.

use lit_billing_auth::BillingAuth;
use lit_billing_core::{StripeClient, customer};
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, get, put};
use serde::Serialize;
use sqlx::PgPool;

use crate::auto_topup::db;
use crate::auto_topup::types::{AutoTopupConfigRow, AutoTopupConfigUpsert};

/// Hard floor on a single top-up amount: $5.00. Matches the existing
/// one-shot manual top-up minimum so users don't get conflicting limits.
const MIN_TOPUP_CENTS: i64 = 500;

/// Glitch's PR review #5: hard ceiling on a single auto top-up charge
/// — $200. Off-session charging (MIT prior-authorization) lets us pull
/// arbitrary amounts without the user re-confirming, so an upper bound
/// is a meaningful guardrail: if a config row got corrupted or an
/// attacker compromised an account and rewrote the row, this caps the
/// blast radius per charge. The monthly cap caps the radius per month.
const MAX_TOPUP_CENTS: i64 = 20_000;

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

/// Shared identity resolution: the dashboard endpoints all need the
/// caller's wallet address + Stripe customer id. Wallet-sig auth gives us
/// the wallet directly; API-key auth needs the on-chain resolver hop
/// (Phase 5+). For now we 501 the API-key path so the dashboard's
/// wallet-sig flow ships unblocked.
async fn resolve_caller(
    auth: &BillingAuth,
    stripe: &StripeClient,
) -> Result<(String, String), (Status, Json<ErrorBody>)> {
    let wallet_address = match auth {
        BillingAuth::WalletSigned {
            wallet_address_hex, ..
        } => wallet_address_hex.clone(),
        BillingAuth::ApiKey(_) => {
            return Err(err(
                Status::NotImplemented,
                "api_key_config_unsupported",
                "API-key callers must use the dashboard wallet-sign flow to manage auto top-up config.",
            ));
        }
    };
    let customer_id = customer::find_by_wallet(stripe, &wallet_address)
        .await
        .map_err(|e| {
            tracing::error!("auto_topup_config: customer lookup failed: {e}");
            err(
                Status::ServiceUnavailable,
                "stripe_lookup_failed",
                "Could not contact Stripe to look up your customer record; try again.",
            )
        })?
        .ok_or_else(|| {
            err(
                Status::BadRequest,
                "no_stripe_customer",
                "Make a one-time top-up first to set up billing, then come back to configure auto top-up.",
            )
        })?;
    Ok((wallet_address, customer_id))
}

#[get("/billing/auto_topup_config")]
pub async fn get_auto_topup_config(
    auth: BillingAuth,
    stripe: &State<StripeClient>,
    pool: &State<PgPool>,
) -> Result<Json<Option<AutoTopupConfigRow>>, (Status, Json<ErrorBody>)> {
    let (_wallet, customer_id) = resolve_caller(&auth, stripe.inner()).await?;
    let row = db::get_by_customer_id(pool.inner(), &customer_id)
        .await
        .map_err(|e| {
            tracing::error!("auto_topup_config GET: db read failed: {e}");
            err(
                Status::ServiceUnavailable,
                "db_unavailable",
                "Could not read your auto top-up config; try again.",
            )
        })?;
    Ok(Json(row))
}

/// Verify `payment_method_id` is attached to the caller's Stripe customer.
///
/// We list the customer's payment methods (paginated; 100/page) and require
/// the requested `pm_xxx` to appear. This is the cheapest cross-tenant
/// guard: even with 100s of cards, the API call cost is bounded, and a
/// caller can only attach a pm_xxx they actually own (Stripe enforces that
/// at the SetupIntent confirm step).
async fn verify_payment_method_owned(
    stripe: &StripeClient,
    customer_id: &str,
    payment_method_id: &str,
) -> Result<bool, anyhow::Error> {
    let resp = stripe
        .get(
            "payment_methods",
            &[
                ("customer", customer_id),
                ("type", "card"),
                ("limit", "100"),
            ],
        )
        .await?;
    let owned = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                .any(|id| id == payment_method_id)
        })
        .unwrap_or(false);
    Ok(owned)
}

#[put("/billing/auto_topup_config", format = "json", data = "<body>")]
pub async fn put_auto_topup_config(
    auth: BillingAuth,
    body: Json<AutoTopupConfigUpsert>,
    stripe: &State<StripeClient>,
    pool: &State<PgPool>,
) -> Result<Json<AutoTopupConfigRow>, (Status, Json<ErrorBody>)> {
    let (wallet_address, customer_id) = resolve_caller(&auth, stripe.inner()).await?;
    let body = body.into_inner();

    // Server-side invariants. The DB CHECK constraint also enforces these,
    // but giving a structured error message is friendlier than surfacing a
    // raw constraint violation.
    if body.enabled {
        let threshold = body.threshold_cents.unwrap_or(0);
        let topup = body.topup_amount_cents.unwrap_or(0);
        let cap = body.monthly_cap_cents.unwrap_or(0);
        if threshold <= 0
            || !(MIN_TOPUP_CENTS..=MAX_TOPUP_CENTS).contains(&topup)
            || topup < threshold
            || cap < topup
        {
            return Err(err(
                Status::BadRequest,
                "invalid_config",
                format!(
                    "Auto top-up requires threshold > 0, \
                     {MIN_TOPUP_CENTS} <= top-up amount <= {MAX_TOPUP_CENTS} cents, \
                     top-up amount >= threshold (so a single charge brings balance back \
                     above threshold), and monthly cap >= top-up amount."
                ),
            ));
        }
        if body.payment_method_id.is_none() {
            return Err(err(
                Status::BadRequest,
                "invalid_config",
                "Auto top-up requires a saved card. Save one first via /billing/setup_intent.",
            ));
        }
        if body.consent_version.is_none() {
            return Err(err(
                Status::BadRequest,
                "invalid_config",
                "Auto top-up requires consent acknowledgement.",
            ));
        }
    }

    // Cross-tenant guard: a wallet must not be able to set a pm_xxx that
    // belongs to a different Stripe customer.
    if let Some(pm) = body.payment_method_id.as_deref() {
        let owned = verify_payment_method_owned(stripe.inner(), &customer_id, pm)
            .await
            .map_err(|e| {
                tracing::error!("auto_topup_config PUT: pm ownership check failed: {e}");
                err(
                    Status::ServiceUnavailable,
                    "stripe_lookup_failed",
                    "Could not verify the payment method; try again.",
                )
            })?;
        if !owned {
            return Err(err(
                Status::BadRequest,
                "payment_method_not_owned",
                "That payment method does not belong to your account.",
            ));
        }
    }

    let row = db::upsert(pool.inner(), &customer_id, &wallet_address, &body)
        .await
        .map_err(|e| {
            if db::is_check_constraint_violation(&e) {
                err(
                    Status::BadRequest,
                    "invalid_config",
                    "Auto top-up requires all of threshold, top-up amount, monthly cap, \
                     a saved card, and consent when enabled.",
                )
            } else {
                tracing::error!("auto_topup_config PUT: upsert failed: {e}");
                err(
                    Status::ServiceUnavailable,
                    "db_unavailable",
                    "Could not save your auto top-up config; try again.",
                )
            }
        })?;
    Ok(Json(row))
}
