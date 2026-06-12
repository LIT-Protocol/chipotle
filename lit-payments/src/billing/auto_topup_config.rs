//! `GET` / `PUT /billing/auto_topup_config` — dashboard CRUD for the
//! per-user auto top-up rule.
//!
//! Behind the [`BillingAuth`] guard. Both wallet-sig and API-key callers
//! are supported: the guard verifies the credential, and the handler
//! re-resolves the API-key path through `AuthResolver` (cached) to pull
//! out the wallet address. Plan §5 says these endpoints sit behind the
//! shared auth module — which already does the verification — so a
//! 501 for API-key callers would contradict the spec.
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

use std::sync::Arc;

use lit_billing_core::billing_auth::{AuthResolver, BillingAuth};
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
/// caller's wallet address + Stripe customer id.
///
/// Wallet-sig auth gives us the wallet directly. API-key auth re-runs
/// `resolve_api_key` (verified by the guard, cached on the resolver
/// side) to pull the wallet out — closes codex's Phase 5 P1 about the
/// 501 short-circuit contradicting plan §5.
async fn resolve_caller(
    auth: &BillingAuth,
    stripe: &StripeClient,
    resolver: &Arc<dyn AuthResolver>,
) -> Result<(String, String), (Status, Json<ErrorBody>)> {
    let wallet_address = match auth {
        BillingAuth::WalletSigned {
            wallet_address_hex, ..
        } => wallet_address_hex.clone(),
        BillingAuth::ApiKey(key) => match resolver.resolve_api_key(key).await {
            Ok(identity) => identity.wallet_address_hex,
            Err(e) => {
                tracing::warn!("auto_topup_config: API-key resolver failed: {e}");
                let (status, code, message): (Status, &'static str, &str) = match e {
                    lit_billing_core::billing_auth::AuthError::BadCredentials(_)
                    | lit_billing_core::billing_auth::AuthError::Forbidden(_) => (
                        Status::Unauthorized,
                        "api_key_unresolved",
                        "API key could not be resolved to a wallet.",
                    ),
                    lit_billing_core::billing_auth::AuthError::Transient(_) => (
                        Status::ServiceUnavailable,
                        "auth_resolver_unavailable",
                        "Could not contact the auth resolver; try again.",
                    ),
                };
                return Err(err(status, code, message));
            }
        },
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
    resolver: &State<Arc<dyn AuthResolver>>,
) -> Result<Json<Option<AutoTopupConfigRow>>, (Status, Json<ErrorBody>)> {
    let (_wallet, customer_id) = resolve_caller(&auth, stripe.inner(), resolver.inner()).await?;
    let mut row = db::get_by_customer_id(pool.inner(), &customer_id)
        .await
        .map_err(|e| {
            tracing::error!("auto_topup_config GET: db read failed: {e}");
            err(
                Status::ServiceUnavailable,
                "db_unavailable",
                "Could not read your auto top-up config; try again.",
            )
        })?;

    // Enrich the response with card brand + last4 so the dashboard can
    // render "Visa •••• 4242" instead of the raw `pm_xxx` id. Stripe is
    // the source of truth — we never persist card metadata locally
    // (PCI scope stays with Stripe). One Stripe call per GET; cheap
    // and bounded because this only runs when the page actively asks.
    if let Some(ref mut r) = row
        && let Some(ref pm_id) = r.payment_method_id
    {
        match stripe
            .inner()
            .get(&format!("payment_methods/{pm_id}"), &[])
            .await
        {
            Ok(resp) => {
                r.card_brand = resp
                    .body
                    .pointer("/card/brand")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                r.card_last4 = resp
                    .body
                    .pointer("/card/last4")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Err(e) => {
                tracing::warn!(
                    payment_method_id = %pm_id,
                    "auto_topup_config GET: card brand/last4 lookup failed (non-fatal): {e}"
                );
            }
        }
    }

    Ok(Json(row))
}

#[derive(Debug, Serialize)]
pub struct PaymentMethodSummary {
    pub payment_method_id: String,
    pub card_brand: String,
    pub card_last4: String,
}

/// `GET /billing/payment_method?pm_id=pm_xxx` — return brand + last4 for
/// a payment method, IF it belongs to the caller's Stripe customer.
/// Used right after the dashboard's "Add a card" flow so the modal can
/// render "Visa •••• 4242" before the user commits the auto-topup row
/// to the DB (i.e. before `GET /billing/auto_topup_config` knows about
/// the new card). Cross-tenant guard reuses `verify_payment_method_owned`.
#[get("/billing/payment_method?<pm_id>")]
pub async fn get_payment_method(
    auth: BillingAuth,
    pm_id: &str,
    stripe: &State<StripeClient>,
    resolver: &State<Arc<dyn AuthResolver>>,
) -> Result<Json<PaymentMethodSummary>, (Status, Json<ErrorBody>)> {
    let (_wallet, customer_id) = resolve_caller(&auth, stripe.inner(), resolver.inner()).await?;
    if pm_id.trim().is_empty() {
        return Err(err(
            Status::BadRequest,
            "missing_pm_id",
            "pm_id query parameter is required.",
        ));
    }
    let owned = verify_payment_method_owned(stripe.inner(), &customer_id, pm_id)
        .await
        .map_err(|e| {
            tracing::error!("payment_method GET: ownership check failed: {e}");
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
    let resp = stripe
        .inner()
        .get(&format!("payment_methods/{pm_id}"), &[])
        .await
        .map_err(|e| {
            tracing::error!("payment_method GET: stripe lookup failed: {e}");
            err(
                Status::ServiceUnavailable,
                "stripe_lookup_failed",
                "Could not retrieve card details; try again.",
            )
        })?;
    let card_brand = resp
        .body
        .pointer("/card/brand")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let card_last4 = resp
        .body
        .pointer("/card/last4")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Json(PaymentMethodSummary {
        payment_method_id: pm_id.to_string(),
        card_brand,
        card_last4,
    }))
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
    resolver: &State<Arc<dyn AuthResolver>>,
) -> Result<Json<AutoTopupConfigRow>, (Status, Json<ErrorBody>)> {
    let (wallet_address, customer_id) =
        resolve_caller(&auth, stripe.inner(), resolver.inner()).await?;
    let body = body.into_inner();

    // Server-side invariants. The DB CHECK constraint also enforces these,
    // but giving a structured error message is friendlier than surfacing a
    // raw constraint violation.
    if body.enabled {
        let threshold = body.threshold_cents.unwrap_or(0);
        let topup = body.topup_amount_cents.unwrap_or(0);
        let cap_opt = body.monthly_cap_cents;
        // Cap is optional: None = unlimited (still bounded per-charge by
        // MAX_TOPUP_CENTS = $200). When Some, must be >= top-up amount.
        let cap_bad = matches!(cap_opt, Some(c) if c < topup);
        if threshold <= 0 || !(MIN_TOPUP_CENTS..=MAX_TOPUP_CENTS).contains(&topup) || cap_bad {
            return Err(err(
                Status::BadRequest,
                "invalid_config",
                format!(
                    "Auto top-up requires threshold > 0, \
                     {MIN_TOPUP_CENTS} <= top-up amount <= {MAX_TOPUP_CENTS} cents, \
                     and (if set) monthly cap >= top-up amount."
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
