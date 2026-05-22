//! LITKEY/USD rate parsing, validation, persistence, admin API, and polling.

use std::time::Duration as StdDuration;

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use crate::auth::Operator;
use crate::auth::operator::Role;
use crate::config::Config;
use crate::portal::types::ErrorResponse;

pub const COINGECKO_URL: &str =
    "https://api.coingecko.com/api/v3/simple/price?ids=lit-protocol&vs_currencies=usd";
pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
pub const STALE_AFTER: Duration = Duration::hours(1);
pub const MAX_CENTS_PER_LITKEY: i64 = 1_000_000; // $10,000/LITKEY: reject obvious feed nonsense.
pub const MAX_DISCOUNT_BASIS_POINTS: i64 = 9_000; // Cap at 90% off to avoid effectively-free credits.

#[derive(Debug, Clone, Serialize)]
pub struct LitkeyRate {
    pub cents_per_litkey: i64,
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
    pub updated_by_operator_id: Option<i64>,
    pub stale: bool,
}

#[derive(Debug, Serialize)]
pub struct RateResponse {
    pub rate: Option<LitkeyRate>,
    pub discount_basis_points: i64,
    pub effective_cents_per_litkey: Option<i64>,
    pub crediting_paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct OverrideRateRequest {
    pub cents_per_litkey: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateValidation {
    Accept,
    RejectInvalid,
    RejectAbsurdJump,
}

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
    tracing::warn!("rate route: {e}");
    err(Status::InternalServerError, "internal error")
}

/// Parse CoinGecko's `simple/price` response and convert USD to integer cents.
pub fn parse_coingecko_cents(body: &str) -> Result<i64> {
    let v: serde_json::Value = serde_json::from_str(body).context("parsing CoinGecko JSON")?;
    let usd = v
        .get("lit-protocol")
        .and_then(|obj| obj.get("usd"))
        .and_then(|usd| usd.as_f64())
        .ok_or_else(|| anyhow!("CoinGecko response missing lit-protocol.usd"))?;

    if !usd.is_finite() || usd <= 0.0 {
        anyhow::bail!("CoinGecko returned non-positive or non-finite USD rate: {usd}");
    }

    let cents = (usd * 100.0).round();
    if !cents.is_finite() || cents < 1.0 || cents > MAX_CENTS_PER_LITKEY as f64 {
        anyhow::bail!("CoinGecko USD rate converts to invalid cents value: {cents}");
    }
    Ok(cents as i64)
}

pub fn validate_manual_cents(cents: i64) -> Result<()> {
    if !(1..=MAX_CENTS_PER_LITKEY).contains(&cents) {
        anyhow::bail!(
            "cents_per_litkey must be between 1 and {}",
            MAX_CENTS_PER_LITKEY
        );
    }
    Ok(())
}

pub fn validate_discount_basis_points(discount_basis_points: i64) -> Result<()> {
    if !(0..=MAX_DISCOUNT_BASIS_POINTS).contains(&discount_basis_points) {
        anyhow::bail!(
            "LITKEY_DISCOUNT_BASIS_POINTS must be between 0 and {} basis points",
            MAX_DISCOUNT_BASIS_POINTS
        );
    }
    Ok(())
}

/// Convert market cents/LITKEY into Stripe credit cents/LITKEY after discount.
///
/// A 20% discount means paying $0.80 worth of LITKEY should buy $1.00 of
/// Stripe credit, so the credit rate is market_rate / (1 - discount).
pub fn apply_discount_to_cents(
    market_cents_per_litkey: i64,
    discount_basis_points: i64,
) -> Result<i64> {
    validate_manual_cents(market_cents_per_litkey)?;
    validate_discount_basis_points(discount_basis_points)?;
    let denominator = 10_000_i128 - i128::from(discount_basis_points);
    let numerator = i128::from(market_cents_per_litkey) * 10_000_i128;
    let rounded = (numerator + denominator / 2) / denominator;
    i64::try_from(rounded).context("discount-adjusted LITKEY cents overflowed i64")
}

/// Reject invalid candidates and feed spikes above 100x the most recent valid row.
pub fn validate_candidate_cents(candidate: i64, recent_cents: Option<i64>) -> RateValidation {
    if validate_manual_cents(candidate).is_err() {
        return RateValidation::RejectInvalid;
    }
    if let Some(recent) = recent_cents.filter(|r| *r > 0)
        && candidate > recent.saturating_mul(100)
    {
        return RateValidation::RejectAbsurdJump;
    }
    RateValidation::Accept
}

pub fn is_stale(fetched_at: OffsetDateTime, now: OffsetDateTime) -> bool {
    now - fetched_at > STALE_AFTER
}

pub fn should_poll_coingecko(current: Option<&LitkeyRate>) -> bool {
    // A manual override is intentionally authoritative until an admin changes
    // it. It may become stale (and therefore pause crediting), but the poller
    // should not automatically overwrite it with the source the admin overrode.
    !matches!(current, Some(rate) if rate.source == "manual")
}

pub async fn fetch_coingecko_cents(client: &Client) -> Result<i64> {
    let body = client
        .get(COINGECKO_URL)
        .send()
        .await
        .context("fetching CoinGecko LIT price")?
        .error_for_status()
        .context("CoinGecko returned non-success status")?
        .text()
        .await
        .context("reading CoinGecko response body")?;
    parse_coingecko_cents(&body)
}

pub async fn get_current(pool: &PgPool) -> Result<Option<LitkeyRate>> {
    let row = sqlx::query_as::<_, (i64, String, OffsetDateTime, Option<i64>)>(
        "SELECT cents_per_litkey, source, fetched_at, updated_by_operator_id \
         FROM litkey_rate WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("selecting litkey_rate")?;

    let now = OffsetDateTime::now_utc();
    Ok(row.map(
        |(cents_per_litkey, source, fetched_at, updated_by_operator_id)| LitkeyRate {
            cents_per_litkey,
            source,
            fetched_at,
            updated_by_operator_id,
            stale: is_stale(fetched_at, now),
        },
    ))
}

pub async fn upsert_coingecko(
    pool: &PgPool,
    cents_per_litkey: i64,
    fetched_at: OffsetDateTime,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO litkey_rate (id, cents_per_litkey, source, fetched_at, updated_by_operator_id) \
         VALUES (1, $1, 'coingecko', $2, NULL) \
         ON CONFLICT (id) DO UPDATE SET \
           cents_per_litkey = EXCLUDED.cents_per_litkey, \
           source = EXCLUDED.source, \
           fetched_at = EXCLUDED.fetched_at, \
           updated_by_operator_id = NULL",
    )
    .bind(cents_per_litkey)
    .bind(fetched_at)
    .execute(pool)
    .await
    .context("upserting CoinGecko litkey_rate")?;
    Ok(())
}

pub async fn set_manual(pool: &PgPool, cents_per_litkey: i64, operator_id: i64) -> Result<()> {
    validate_manual_cents(cents_per_litkey)?;
    sqlx::query(
        "INSERT INTO litkey_rate (id, cents_per_litkey, source, fetched_at, updated_by_operator_id) \
         VALUES (1, $1, 'manual', now(), $2) \
         ON CONFLICT (id) DO UPDATE SET \
           cents_per_litkey = EXCLUDED.cents_per_litkey, \
           source = EXCLUDED.source, \
           fetched_at = EXCLUDED.fetched_at, \
           updated_by_operator_id = EXCLUDED.updated_by_operator_id",
    )
    .bind(cents_per_litkey)
    .bind(operator_id)
    .execute(pool)
    .await
    .context("setting manual litkey_rate")?;
    Ok(())
}

fn coingecko_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(StdDuration::from_secs(5))
        .timeout(StdDuration::from_secs(10))
        .build()
        .context("building CoinGecko HTTP client")
}

pub fn spawn_rate_poller(pool: PgPool) {
    tokio::spawn(async move {
        let client = match coingecko_client() {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!("litkey rate poller could not start: {e}");
                return;
            }
        };
        loop {
            poll_once(&pool, &client).await;
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

async fn poll_once(pool: &PgPool, client: &Client) {
    let current = match get_current(pool).await {
        Ok(rate) => rate,
        Err(e) => {
            tracing::warn!("litkey rate poller could not read current rate: {e}");
            None
        }
    };

    if !should_poll_coingecko(current.as_ref()) {
        tracing::info!("LITKEY rate poller skipping CoinGecko; fresh manual override is active");
        return;
    }

    match fetch_coingecko_cents(client).await {
        Ok(cents) => {
            match validate_candidate_cents(cents, current.as_ref().map(|r| r.cents_per_litkey)) {
                RateValidation::Accept => {
                    if let Err(e) = upsert_coingecko(pool, cents, OffsetDateTime::now_utc()).await {
                        tracing::warn!("litkey rate poller failed to save CoinGecko rate: {e}");
                    } else {
                        tracing::info!(
                            cents_per_litkey = cents,
                            "updated LITKEY rate from CoinGecko"
                        );
                    }
                }
                RateValidation::RejectInvalid | RateValidation::RejectAbsurdJump => {
                    tracing::warn!(
                        candidate_cents = cents,
                        recent_cents = current.as_ref().map(|r| r.cents_per_litkey),
                        "CoinGecko LITKEY rate looked invalid/absurd; keeping last-known rate"
                    );
                }
            }
        }
        Err(e) => tracing::warn!("litkey rate poller fetch failed; keeping last-known rate: {e}"),
    }

    if let Ok(Some(rate)) = get_current(pool).await
        && rate.stale
    {
        tracing::warn!(
            fetched_at = %rate.fetched_at,
            cents_per_litkey = rate.cents_per_litkey,
            source = %rate.source,
            "LITKEY rate is stale (>1h); future payment crediting must pause"
        );
    }
}

/// `GET /api/litkey/rate` — current LITKEY/USD rate, if one exists.
#[rocket::get("/api/litkey/rate")]
pub async fn get_rate(
    _operator: Operator,
    pool: &State<PgPool>,
    config: &State<Config>,
) -> ApiResult<RateResponse> {
    let rate = get_current(pool).await.map_err(server_err)?;
    Ok(Json(build_rate_response(
        rate,
        config.litkey_discount_basis_points,
    )?))
}

/// `GET /api/litkey/quote` — public quote for the end-user pay-with-LITKEY page.
#[rocket::get("/api/litkey/quote")]
pub async fn get_quote(pool: &State<PgPool>, config: &State<Config>) -> ApiResult<RateResponse> {
    let rate = get_current(pool).await.map_err(server_err)?;
    Ok(Json(build_rate_response(
        rate,
        config.litkey_discount_basis_points,
    )?))
}

/// `POST /api/litkey/rate/override` — admin-only manual rate override.
#[rocket::post("/api/litkey/rate/override", format = "json", data = "<req>")]
pub async fn override_rate(
    operator: Operator,
    req: Json<OverrideRateRequest>,
    pool: &State<PgPool>,
    config: &State<Config>,
) -> ApiResult<RateResponse> {
    if operator.role != Role::Admin {
        return Err(err(Status::Forbidden, "admin role required"));
    }
    let cents = req.cents_per_litkey;
    validate_manual_cents(cents).map_err(|e| err(Status::BadRequest, e.to_string()))?;
    set_manual(pool, cents, operator.id)
        .await
        .map_err(server_err)?;
    let rate = get_current(pool).await.map_err(server_err)?;
    Ok(Json(build_rate_response(
        rate,
        config.litkey_discount_basis_points,
    )?))
}

fn build_rate_response(
    rate: Option<LitkeyRate>,
    discount_basis_points: i64,
) -> Result<RateResponse, ApiError> {
    let crediting_paused = rate.as_ref().is_none_or(|r| r.stale);
    let effective_cents_per_litkey = rate
        .as_ref()
        .filter(|_| !crediting_paused)
        .map(|r| apply_discount_to_cents(r.cents_per_litkey, discount_basis_points))
        .transpose()
        .map_err(server_err)?;
    Ok(RateResponse {
        rate,
        discount_basis_points,
        effective_cents_per_litkey,
        crediting_paused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, OffsetDateTime};

    #[test]
    fn parses_coingecko_usd_to_integer_cents_with_rounding() {
        assert_eq!(
            parse_coingecko_cents(r#"{"lit-protocol":{"usd":0.124}}"#).unwrap(),
            12
        );
        assert_eq!(
            parse_coingecko_cents(r#"{"lit-protocol":{"usd":0.125}}"#).unwrap(),
            13
        );
    }

    #[test]
    fn rejects_missing_zero_negative_non_finite_and_absurd_rates() {
        assert!(parse_coingecko_cents(r#"{}"#).is_err());
        assert!(parse_coingecko_cents(r#"{"lit-protocol":{"usd":0}}"#).is_err());
        assert!(parse_coingecko_cents(r#"{"lit-protocol":{"usd":-1}}"#).is_err());
        assert!(parse_coingecko_cents(r#"{"lit-protocol":{"usd":1e309}}"#).is_err());
        assert!(parse_coingecko_cents(r#"{"lit-protocol":{"usd":10001}}"#).is_err());
    }

    #[test]
    fn rejects_candidate_more_than_100x_recent_rate() {
        assert_eq!(
            validate_candidate_cents(1_001, Some(10)),
            RateValidation::RejectAbsurdJump
        );
        assert_eq!(
            validate_candidate_cents(1_000, Some(10)),
            RateValidation::Accept
        );
        assert_eq!(validate_candidate_cents(50, None), RateValidation::Accept);
    }

    #[test]
    fn stale_is_strictly_older_than_one_hour() {
        let now = OffsetDateTime::now_utc();
        assert!(!is_stale(now - Duration::hours(1), now));
        assert!(is_stale(
            now - Duration::hours(1) - Duration::seconds(1),
            now
        ));
    }

    #[test]
    fn manual_override_pauses_coingecko_polling_until_admin_changes_it() {
        let now = OffsetDateTime::now_utc();
        let fresh_manual = LitkeyRate {
            cents_per_litkey: 12,
            source: "manual".to_string(),
            fetched_at: now,
            updated_by_operator_id: Some(1),
            stale: false,
        };
        let stale_manual = LitkeyRate {
            stale: true,
            ..fresh_manual.clone()
        };
        let coingecko = LitkeyRate {
            source: "coingecko".to_string(),
            updated_by_operator_id: None,
            stale: false,
            ..fresh_manual.clone()
        };

        assert!(!should_poll_coingecko(Some(&fresh_manual)));
        assert!(!should_poll_coingecko(Some(&stale_manual)));
        assert!(should_poll_coingecko(Some(&coingecko)));
        assert!(should_poll_coingecko(None));
    }

    #[test]
    fn stale_or_missing_rate_pauses_crediting_and_hides_effective_rate() {
        let now = OffsetDateTime::now_utc();
        let stale_rate = LitkeyRate {
            cents_per_litkey: 100,
            source: "coingecko".to_string(),
            fetched_at: now - Duration::hours(2),
            updated_by_operator_id: None,
            stale: true,
        };

        let missing = build_rate_response(None, 2_000).unwrap();
        assert!(missing.crediting_paused);
        assert_eq!(missing.effective_cents_per_litkey, None);

        let stale = build_rate_response(Some(stale_rate), 2_000).unwrap();
        assert!(stale.crediting_paused);
        assert_eq!(stale.effective_cents_per_litkey, None);
    }

    #[test]
    fn fresh_rate_exposes_discount_adjusted_effective_rate() {
        let now = OffsetDateTime::now_utc();
        let fresh_rate = LitkeyRate {
            cents_per_litkey: 100,
            source: "coingecko".to_string(),
            fetched_at: now,
            updated_by_operator_id: None,
            stale: false,
        };

        let response = build_rate_response(Some(fresh_rate), 2_000).unwrap();
        assert!(!response.crediting_paused);
        assert_eq!(response.effective_cents_per_litkey, Some(125));
    }

    #[test]
    fn manual_override_validation_accepts_positive_reasonable_cents_only() {
        assert!(validate_manual_cents(1).is_ok());
        assert!(validate_manual_cents(1_000_000).is_ok());
        assert!(validate_manual_cents(0).is_err());
        assert!(validate_manual_cents(1_000_001).is_err());
    }

    #[test]
    fn applies_litkey_discount_as_extra_credit_per_token() {
        assert_eq!(apply_discount_to_cents(100, 0).unwrap(), 100);
        // 20% off means paying $0.80 worth of LITKEY buys $1.00 credit,
        // so each token credits 1 / (1 - 0.20) = 1.25x spot value.
        assert_eq!(apply_discount_to_cents(100, 2_000).unwrap(), 125);
        assert_eq!(apply_discount_to_cents(33, 2_000).unwrap(), 41);
    }

    #[test]
    fn validates_litkey_discount_basis_points() {
        assert!(validate_discount_basis_points(0).is_ok());
        assert!(validate_discount_basis_points(2_000).is_ok());
        assert!(validate_discount_basis_points(9_000).is_ok());
        assert!(validate_discount_basis_points(-1).is_err());
        assert!(validate_discount_basis_points(9_001).is_err());
    }
}
