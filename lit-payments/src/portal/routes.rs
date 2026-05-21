//! Admin credit portal routes: lookup, grant, recent grants.
//!
//! All routes are gated by the [`Operator`] request guard so an unauthenticated
//! caller forwards into the 401 catcher.

use lit_billing_core::StripeClient;
use lit_billing_core::balance;
use lit_billing_core::customer;
use lit_billing_core::format::cents_to_display;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, post};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::caps::{self, CapCheck};
use super::db;
use super::types::{
    CustomerMatch, ErrorResponse, GrantRequest, GrantResponse, GrantRow, GrantsResponse,
    LookupResponse,
};
use crate::auth::Operator;
use crate::config::Config;

const DEFAULT_GRANTS_LIMIT: i64 = 50;
const MAX_GRANTS_LIMIT: i64 = 200;

type ApiError = (Status, Json<ErrorResponse>);
type ApiResult<T> = Result<Json<T>, ApiError>;

fn err(status: Status, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

fn server_err(e: impl std::fmt::Display) -> ApiError {
    tracing::warn!("portal route: {e}");
    err(Status::InternalServerError, "internal error")
}

fn balance_display(balance_cents: i64) -> String {
    let credits = -balance_cents;
    if credits <= 0 {
        "No credits".to_string()
    } else {
        format!("{} credit", cents_to_display(credits))
    }
}

async fn build_match(
    stripe: &StripeClient,
    stripe_customer_id: String,
    email: Option<String>,
    wallet_address: Option<String>,
) -> Result<CustomerMatch, ApiError> {
    let balance_cents = balance::fetch(stripe, &stripe_customer_id)
        .await
        .map_err(server_err)?;
    Ok(CustomerMatch {
        stripe_customer_id,
        email,
        wallet_address,
        balance_cents,
        balance_display: balance_display(balance_cents),
    })
}

/// `GET /api/customer/lookup?email=…` or `?wallet=…`
///
/// Exactly one of `email` or `wallet` must be supplied. Returns zero or more
/// matches with the current balance attached. Email search may return multiple
/// matches; wallet search returns at most one.
#[get("/api/customer/lookup?<email>&<wallet>")]
pub async fn lookup_customer(
    _operator: Operator,
    email: Option<&str>,
    wallet: Option<&str>,
    stripe: &State<StripeClient>,
) -> ApiResult<LookupResponse> {
    let email = email.map(str::trim).filter(|s| !s.is_empty());
    let wallet = wallet.map(str::trim).filter(|s| !s.is_empty());

    match (email, wallet) {
        (None, None) | (Some(_), Some(_)) => Err(err(
            Status::BadRequest,
            "exactly one of `email` or `wallet` is required",
        )),
        (Some(email), None) => {
            let summaries = customer::search_by_email(stripe, email)
                .await
                .map_err(server_err)?;
            let mut matches = Vec::with_capacity(summaries.len());
            for s in summaries {
                matches.push(build_match(stripe, s.id, s.email, s.wallet_address).await?);
            }
            Ok(Json(LookupResponse { matches }))
        }
        (None, Some(wallet)) => {
            let id = customer::find_by_wallet(stripe, wallet)
                .await
                .map_err(server_err)?;
            let matches = match id {
                Some(id) => {
                    let m = build_match(stripe, id, None, Some(wallet.to_string())).await?;
                    vec![m]
                }
                None => vec![],
            };
            Ok(Json(LookupResponse { matches }))
        }
    }
}

/// `POST /api/grant` — apply a credit to a customer's Stripe balance.
///
/// Flow:
/// 1. Validate idempotency-key format + cents > 0.
/// 2. Replay short-circuit: if a grant with this idempotency key already
///    exists, return it without re-hitting Stripe.
/// 3. Cap check (per-grant + per-operator-per-day).
/// 4. Resolve wallet → Stripe customer. 404 if no customer exists for the
///    wallet (we don't auto-create from the admin portal).
/// 5. Write the Stripe balance_transaction with idempotency key so retries
///    after a network error don't duplicate the credit upstream.
/// 6. Insert the grants row (ON CONFLICT DO NOTHING — race-safe).
/// 7. Re-fetch the balance and return.
#[post("/api/grant", format = "json", data = "<req>")]
pub async fn grant_credit(
    operator: Operator,
    req: Json<GrantRequest>,
    pool: &State<PgPool>,
    stripe: &State<StripeClient>,
    config: &State<Config>,
) -> ApiResult<GrantResponse> {
    let req = req.into_inner();

    // Step 1: input validation.
    if Uuid::parse_str(&req.idempotency_key).is_err() {
        return Err(err(Status::BadRequest, "idempotency_key must be a UUID"));
    }
    if req.cents <= 0 {
        return Err(err(Status::BadRequest, "cents must be positive"));
    }
    let wallet = req.wallet_address.trim();
    if wallet.is_empty() {
        return Err(err(Status::BadRequest, "wallet_address is required"));
    }
    let note = req.note.trim();

    // Step 2: idempotent replay short-circuit.
    if let Some(existing) = db::find_by_idempotency_key(pool, &req.idempotency_key)
        .await
        .map_err(server_err)?
    {
        let balance_cents = balance::fetch(stripe, &existing.stripe_customer_id)
            .await
            .map_err(server_err)?;
        return Ok(Json(GrantResponse {
            grant_id: existing.id,
            stripe_customer_id: existing.stripe_customer_id,
            stripe_balance_transaction_id: existing.stripe_balance_transaction_id,
            cents: existing.cents,
            new_balance_cents: balance_cents,
            new_balance_display: balance_display(balance_cents),
            idempotent_replay: true,
        }));
    }

    // Step 3: cap check.
    let already_today = caps::cents_granted_last_24h(pool, operator.id)
        .await
        .map_err(server_err)?;
    let check = caps::check(
        req.cents,
        already_today,
        config.max_grant_cents,
        config.max_daily_per_operator_cents,
    );
    if let CapCheck::NonPositive | CapCheck::OverPerGrant { .. } | CapCheck::OverDaily { .. } =
        &check
    {
        let msg = check.message().unwrap_or_else(|| "cap violation".into());
        return Err(err(Status::BadRequest, msg));
    }

    // Step 4: resolve wallet → Stripe customer.
    let stripe_customer_id = customer::find_by_wallet(stripe, wallet)
        .await
        .map_err(server_err)?
        .ok_or_else(|| {
            err(
                Status::NotFound,
                "no Stripe customer for that wallet — ask the user to top up at least \
                 once via the dashboard so their account is created",
            )
        })?;

    // Step 5: write the Stripe credit.
    let now_iso = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());
    let description = format!(
        "Admin grant: {} (op:{}, {})",
        if note.is_empty() { "[no note]" } else { note },
        operator.email,
        now_iso
    );
    let stripe_balance_transaction_id = balance::write_transaction(
        stripe,
        &stripe_customer_id,
        -req.cents, // negative = credit to customer
        &description,
        Some(&req.idempotency_key),
    )
    .await
    .map_err(server_err)?;

    // Step 6: persist the grants row. Race-safe via ON CONFLICT DO NOTHING;
    // a concurrent retry that won the race produces None here, in which case
    // we fetch the winning row.
    let email_for_db = req
        .email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let new = db::NewGrant {
        operator_id: operator.id,
        stripe_customer_id: &stripe_customer_id,
        wallet_address: wallet,
        email: email_for_db,
        cents: req.cents,
        note,
        stripe_balance_transaction_id: &stripe_balance_transaction_id,
        idempotency_key: &req.idempotency_key,
    };
    let (grant_id, replay) = match db::insert(pool, &new).await.map_err(server_err)? {
        Some((id, _created_at)) => (id, false),
        None => {
            // Concurrent retry beat us; fetch the winning row.
            let existing = db::find_by_idempotency_key(pool, &req.idempotency_key)
                .await
                .map_err(server_err)?
                .ok_or_else(|| {
                    server_err("insert hit ON CONFLICT but follow-up SELECT found nothing")
                })?;
            (existing.id, true)
        }
    };

    // Step 7: read the post-credit balance for the UI.
    let balance_cents = balance::fetch(stripe, &stripe_customer_id)
        .await
        .map_err(server_err)?;

    Ok(Json(GrantResponse {
        grant_id,
        stripe_customer_id,
        stripe_balance_transaction_id,
        cents: req.cents,
        new_balance_cents: balance_cents,
        new_balance_display: balance_display(balance_cents),
        idempotent_replay: replay,
    }))
}

/// `GET /api/grants?limit=N` — recent grants by the calling operator, newest
/// first. Default 50, max 200.
#[get("/api/grants?<limit>")]
pub async fn list_grants(
    operator: Operator,
    limit: Option<i64>,
    pool: &State<PgPool>,
) -> ApiResult<GrantsResponse> {
    let limit = limit
        .unwrap_or(DEFAULT_GRANTS_LIMIT)
        .clamp(1, MAX_GRANTS_LIMIT);
    let grants: Vec<GrantRow> = db::list_recent_by_operator(pool, operator.id, limit)
        .await
        .map_err(server_err)?;
    Ok(Json(GrantsResponse { grants }))
}
