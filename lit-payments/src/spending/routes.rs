//! Spending-rules HTTP routes.
//!
//! Operator-authed CRUD under `/api/spending-rules` (browser admin UI) and
//! `ServiceAuth`-authed endpoints under `/internal` (the gateway).

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{State, delete, get, post, put};
use sqlx::PgPool;

use super::db;
use super::types::{
    ChargeRequest, DeleteResponse, ErrorResponse, RulesListResponse, RulesWithUsage, SpendingRules,
    SpendingUsage, UpsertRulesRequest,
};
use super::{ServiceAuth, canonical_key_hash};
use crate::auth::Operator;

const DEFAULT_RULES_LIMIT: i64 = 100;
const MAX_RULES_LIMIT: i64 = 500;

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

fn server_err(e: impl std::fmt::Display + std::fmt::Debug) -> ApiError {
    tracing::warn!(error = %e, error_debug = ?e, "spending route internal error");
    err(Status::InternalServerError, "internal error")
}

fn parse_hash(raw: &str) -> Result<String, ApiError> {
    canonical_key_hash(raw).map_err(|e| err(Status::BadRequest, e))
}

// ─── Operator (admin UI) ────────────────────────────────────────────────────

/// `PUT /api/spending-rules/<hash>` — create or replace a key's rules.
#[put("/api/spending-rules/<api_key_hash>", format = "json", data = "<req>")]
pub async fn put_rules(
    _operator: Operator,
    api_key_hash: &str,
    req: Json<UpsertRulesRequest>,
    pool: &State<PgPool>,
) -> ApiResult<SpendingRules> {
    let hash = parse_hash(api_key_hash)?;
    let req = req.into_inner();
    req.validate().map_err(|e| err(Status::BadRequest, e))?;
    let rules = db::upsert_rules(pool, &hash, &req)
        .await
        .map_err(server_err)?;
    Ok(Json(rules))
}

/// `GET /api/spending-rules/<hash>` — a key's rules + current usage.
#[get("/api/spending-rules/<api_key_hash>")]
pub async fn get_rules(
    _operator: Operator,
    api_key_hash: &str,
    pool: &State<PgPool>,
) -> ApiResult<RulesWithUsage> {
    let hash = parse_hash(api_key_hash)?;
    let rules = db::get_rules(pool, &hash)
        .await
        .map_err(server_err)?
        .ok_or_else(|| err(Status::NotFound, "no rules for that key"))?;
    let usage = db::get_usage(pool, &hash).await.map_err(server_err)?;
    Ok(Json(RulesWithUsage { rules, usage }))
}

/// `GET /api/spending-rules?limit=N` — recently-updated rules, newest first.
#[get("/api/spending-rules?<limit>")]
pub async fn list_rules(
    _operator: Operator,
    limit: Option<i64>,
    pool: &State<PgPool>,
) -> ApiResult<RulesListResponse> {
    let limit = limit.unwrap_or(DEFAULT_RULES_LIMIT).clamp(1, MAX_RULES_LIMIT);
    let rules = db::list_rules(pool, limit).await.map_err(server_err)?;
    Ok(Json(RulesListResponse { rules }))
}

/// `DELETE /api/spending-rules/<hash>` — clear a key's rules + usage counter.
#[delete("/api/spending-rules/<api_key_hash>")]
pub async fn delete_rules(
    _operator: Operator,
    api_key_hash: &str,
    pool: &State<PgPool>,
) -> ApiResult<DeleteResponse> {
    let hash = parse_hash(api_key_hash)?;
    let deleted = db::delete_rules(pool, &hash).await.map_err(server_err)?;
    Ok(Json(DeleteResponse { deleted }))
}

// ─── Internal (gateway) ─────────────────────────────────────────────────────

/// `GET /internal/spending-rules/<hash>` — rules + usage for the gateway's
/// cache. 404 when the key has no rules (the gateway caches that as "no rules").
#[get("/internal/spending-rules/<api_key_hash>")]
pub async fn internal_get_rules(
    _svc: ServiceAuth,
    api_key_hash: &str,
    pool: &State<PgPool>,
) -> ApiResult<RulesWithUsage> {
    let hash = parse_hash(api_key_hash)?;
    let rules = db::get_rules(pool, &hash)
        .await
        .map_err(server_err)?
        .ok_or_else(|| err(Status::NotFound, "no rules for that key"))?;
    let usage = db::get_usage(pool, &hash).await.map_err(server_err)?;
    Ok(Json(RulesWithUsage { rules, usage }))
}

/// `POST /internal/spending-usage/<hash>/charge` — add to the rolling spend
/// counter (resetting the window if elapsed). Called by the gateway off the
/// response path; best-effort.
#[post(
    "/internal/spending-usage/<api_key_hash>/charge",
    format = "json",
    data = "<req>"
)]
pub async fn internal_charge(
    _svc: ServiceAuth,
    api_key_hash: &str,
    req: Json<ChargeRequest>,
    pool: &State<PgPool>,
) -> ApiResult<SpendingUsage> {
    let hash = parse_hash(api_key_hash)?;
    let req = req.into_inner();
    if req.cents <= 0 {
        return Err(err(Status::BadRequest, "cents must be positive"));
    }
    if req.window_seconds <= 0 {
        return Err(err(Status::BadRequest, "window_seconds must be positive"));
    }
    let usage = db::record_charge(pool, &hash, req.cents, req.window_seconds)
        .await
        .map_err(server_err)?;
    Ok(Json(usage))
}
