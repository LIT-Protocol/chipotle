//! LITKEY/USD rate parsing, validation, persistence, admin API, and polling.

use std::str::FromStr;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result, anyhow};
use num_bigint::BigUint;
use num_traits::ToPrimitive;
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

pub const LITKEY_BASE_TOKEN_ADDRESS: &str = "0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49";
pub const COINGECKO_URL: &str = "https://api.coingecko.com/api/v3/simple/token_price/base?contract_addresses=0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49&vs_currencies=usd";
pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
pub const STALE_AFTER: Duration = Duration::hours(1);
pub const MAX_USD_WEI_PER_LITKEY: &str = "10000000000000000000000"; // $10,000/LITKEY scaled by 1e18: reject obvious feed nonsense.
pub const MAX_DISCOUNT_BASIS_POINTS: i64 = 9_000; // Cap at 90% off to avoid effectively-free credits.

#[derive(Debug, Clone, Serialize)]
pub struct LitkeyRate {
    pub usd_wei_per_litkey: String,
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
    pub effective_usd_wei_per_litkey: Option<String>,
    pub crediting_paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct OverrideRateRequest {
    pub usd_wei_per_litkey: String,
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

fn server_err(e: impl std::fmt::Display + std::fmt::Debug) -> ApiError {
    tracing::warn!(error = %e, error_debug = ?e, "rate route internal error");
    err(Status::InternalServerError, "internal error")
}

const USD_FIXED_POINT_DECIMALS: usize = 18;
const USD_FIXED_POINT_SCALE: &str = "1000000000000000000";
const LITKEY_WEI_PER_TOKEN: &str = "1000000000000000000";

fn max_usd_wei_per_litkey() -> BigUint {
    BigUint::from_str(MAX_USD_WEI_PER_LITKEY).expect("valid MAX_USD_WEI_PER_LITKEY")
}

fn usd_scale() -> BigUint {
    BigUint::from_str(USD_FIXED_POINT_SCALE).expect("valid USD_FIXED_POINT_SCALE")
}

fn litkey_scale() -> BigUint {
    BigUint::from_str(LITKEY_WEI_PER_TOKEN).expect("valid LITKEY_WEI_PER_TOKEN")
}

fn parse_biguint_decimal(raw: &str, field: &str) -> Result<BigUint> {
    if raw.is_empty() || raw.starts_with('+') || raw.starts_with('-') {
        anyhow::bail!("{field} must be an unsigned integer string");
    }
    if !raw.bytes().all(|b| b.is_ascii_digit()) {
        anyhow::bail!("{field} must contain only decimal digits");
    }
    BigUint::from_str(raw).with_context(|| format!("parsing {field}"))
}

fn parse_decimal_to_scaled_units(raw: &str, decimals: usize) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('+') || raw.starts_with('-') {
        anyhow::bail!("USD rate must be positive, got {raw:?}");
    }

    let (mantissa, exponent) = if let Some((mantissa, exponent)) = raw.split_once(['e', 'E']) {
        let exponent = exponent
            .parse::<i32>()
            .with_context(|| format!("USD rate has invalid exponent: {raw:?}"))?;
        (mantissa, exponent)
    } else {
        (raw, 0)
    };

    let (whole, fractional) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fractional.is_empty() {
        anyhow::bail!("USD rate is empty");
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !fractional.bytes().all(|b| b.is_ascii_digit())
    {
        anyhow::bail!(
            "USD rate must contain only digits, one decimal point, and optional exponent"
        );
    }

    let mut digits = format!("{whole}{fractional}");
    digits = digits.trim_start_matches('0').to_string();
    if digits.is_empty() {
        anyhow::bail!("USD rate is zero");
    }

    let power = exponent - i32::try_from(fractional.len()).expect("fraction length fits i32")
        + i32::try_from(decimals).expect("scale decimals fit i32");
    let units = if power >= 0 {
        format!("{}{}", digits, "0".repeat(power as usize))
    } else {
        let keep_len = digits.len().saturating_sub((-power) as usize);
        digits[..keep_len].to_string()
    };
    let units = units.trim_start_matches('0');
    if units.is_empty() {
        anyhow::bail!("USD rate rounds/truncates to zero at {decimals} decimals");
    }
    Ok(units.to_string())
}

/// Parse CoinGecko's `simple/price` response and convert USD to integer
/// 18-decimal USD fixed-point units per 1 whole LITKEY.
///
/// Example: `$0.006/LITKEY` becomes `6000000000000000`.
pub fn parse_coingecko_usd_wei(body: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(body).context("parsing CoinGecko JSON")?;
    let usd = v
        .get(LITKEY_BASE_TOKEN_ADDRESS)
        .or_else(|| v.get("lit-protocol"))
        .and_then(|obj| obj.get("usd"))
        .ok_or_else(|| {
            anyhow!(
                "CoinGecko response missing {}.usd / lit-protocol.usd",
                LITKEY_BASE_TOKEN_ADDRESS
            )
        })?;
    let usd = match usd {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => anyhow::bail!("CoinGecko lit-protocol.usd was not a number"),
    };
    let usd_wei = parse_decimal_to_scaled_units(&usd, USD_FIXED_POINT_DECIMALS)?;
    validate_manual_usd_wei(&usd_wei)?;
    Ok(usd_wei)
}

pub fn validate_manual_usd_wei(usd_wei_per_litkey: &str) -> Result<()> {
    let value = parse_biguint_decimal(usd_wei_per_litkey, "usd_wei_per_litkey")?;
    if value == BigUint::from(0_u8) || value > max_usd_wei_per_litkey() {
        anyhow::bail!(
            "usd_wei_per_litkey must be between 1 and {}",
            MAX_USD_WEI_PER_LITKEY
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

fn div_round_half_up(numerator: BigUint, denominator: BigUint) -> BigUint {
    (&numerator + (&denominator / 2_u8)) / denominator
}

/// Convert market USD wei/LITKEY into effective credit USD wei/LITKEY after
/// the configured discount. A 20% discount means paying $0.80 worth of
/// LITKEY buys $1.00 Stripe credit, so effective rate is market/(1-discount).
pub fn apply_discount_to_usd_wei(
    market_usd_wei_per_litkey: &str,
    discount_basis_points: i64,
) -> Result<String> {
    validate_manual_usd_wei(market_usd_wei_per_litkey)?;
    validate_discount_basis_points(discount_basis_points)?;
    let denominator_bps = 10_000_i64 - discount_basis_points;
    let numerator = parse_biguint_decimal(market_usd_wei_per_litkey, "usd_wei_per_litkey")?
        * BigUint::from(10_000_u32);
    let rounded = div_round_half_up(numerator, BigUint::from(denominator_bps as u32));
    Ok(rounded.to_string())
}

/// Compute final Stripe credit cents from native token units and 18-decimal
/// USD fixed-point price. This is the settlement helper for the future
/// listener: keep LITKEY wei and USD wei as integers, apply discount, then
/// round once at the final Stripe-cent boundary.
pub fn litkey_wei_to_credit_cents(
    litkey_amount_wei: &str,
    usd_wei_per_litkey: &str,
    discount_basis_points: i64,
) -> Result<i64> {
    validate_manual_usd_wei(usd_wei_per_litkey)?;
    validate_discount_basis_points(discount_basis_points)?;
    let denominator_bps = 10_000_i64 - discount_basis_points;
    let numerator = parse_biguint_decimal(litkey_amount_wei, "litkey_amount_wei")?
        * parse_biguint_decimal(usd_wei_per_litkey, "usd_wei_per_litkey")?
        * BigUint::from(100_u32)
        * BigUint::from(10_000_u32);
    let denominator = litkey_scale() * usd_scale() * BigUint::from(denominator_bps as u32);
    let cents = div_round_half_up(numerator, denominator);
    cents
        .to_i64()
        .context("computed Stripe cents overflowed i64")
}

/// Reject invalid candidates and feed spikes above 100x the most recent valid row.
pub fn validate_candidate_usd_wei(candidate: &str, recent_usd_wei: Option<&str>) -> RateValidation {
    if validate_manual_usd_wei(candidate).is_err() {
        return RateValidation::RejectInvalid;
    }
    if let Some(recent) = recent_usd_wei {
        let candidate = parse_biguint_decimal(candidate, "candidate usd_wei_per_litkey")
            .expect("already validated candidate");
        if let Ok(recent) = parse_biguint_decimal(recent, "recent usd_wei_per_litkey")
            && candidate > recent * BigUint::from(100_u32)
        {
            return RateValidation::RejectAbsurdJump;
        }
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

pub async fn fetch_coingecko_usd_wei(client: &Client) -> Result<String> {
    let resp = client
        .get(COINGECKO_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .context("fetching CoinGecko LIT price")?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("reading CoinGecko response body")?;
    if !status.is_success() {
        anyhow::bail!(
            "CoinGecko returned non-success status {status}: {}",
            truncate_for_log(&body, 500)
        );
    }
    parse_coingecko_usd_wei(&body)
}

fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let mut out = s.chars().take(max_chars).collect::<String>();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

pub async fn get_current(pool: &PgPool) -> Result<Option<LitkeyRate>> {
    let row = sqlx::query_as::<_, (String, String, OffsetDateTime, Option<i64>)>(
        "SELECT usd_wei_per_litkey::text, source, fetched_at, updated_by_operator_id \
         FROM litkey_rate WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("selecting litkey_rate")?;

    let now = OffsetDateTime::now_utc();
    Ok(row.map(
        |(usd_wei_per_litkey, source, fetched_at, updated_by_operator_id)| LitkeyRate {
            usd_wei_per_litkey,
            source,
            fetched_at,
            updated_by_operator_id,
            stale: is_stale(fetched_at, now),
        },
    ))
}

pub async fn upsert_coingecko(
    pool: &PgPool,
    usd_wei_per_litkey: &str,
    fetched_at: OffsetDateTime,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO litkey_rate (id, usd_wei_per_litkey, source, fetched_at, updated_by_operator_id) \
         VALUES (1, $1::numeric, 'coingecko', $2, NULL) \
         ON CONFLICT (id) DO UPDATE SET \
           usd_wei_per_litkey = EXCLUDED.usd_wei_per_litkey, \
           source = EXCLUDED.source, \
           fetched_at = EXCLUDED.fetched_at, \
           updated_by_operator_id = NULL",
    )
    .bind(usd_wei_per_litkey)
    .bind(fetched_at)
    .execute(pool)
    .await
    .context("upserting CoinGecko litkey_rate")?;
    Ok(())
}

pub async fn set_manual(pool: &PgPool, usd_wei_per_litkey: &str, operator_id: i64) -> Result<()> {
    validate_manual_usd_wei(usd_wei_per_litkey)?;
    sqlx::query(
        "INSERT INTO litkey_rate (id, usd_wei_per_litkey, source, fetched_at, updated_by_operator_id) \
         VALUES (1, $1::numeric, 'manual', now(), $2) \
         ON CONFLICT (id) DO UPDATE SET \
           usd_wei_per_litkey = EXCLUDED.usd_wei_per_litkey, \
           source = EXCLUDED.source, \
           fetched_at = EXCLUDED.fetched_at, \
           updated_by_operator_id = EXCLUDED.updated_by_operator_id",
    )
    .bind(usd_wei_per_litkey)
    .bind(operator_id)
    .execute(pool)
    .await
    .context("setting manual litkey_rate")?;
    Ok(())
}

fn coingecko_client() -> Result<Client> {
    Client::builder()
        .user_agent("lit-payments/0.1 (+https://litprotocol.com)")
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

    match fetch_coingecko_usd_wei(client).await {
        Ok(usd_wei) => {
            match validate_candidate_usd_wei(
                &usd_wei,
                current.as_ref().map(|r| r.usd_wei_per_litkey.as_str()),
            ) {
                RateValidation::Accept => {
                    if let Err(e) =
                        upsert_coingecko(pool, &usd_wei, OffsetDateTime::now_utc()).await
                    {
                        tracing::warn!("litkey rate poller failed to save CoinGecko rate: {e}");
                    } else {
                        tracing::info!(
                            usd_wei_per_litkey = %usd_wei,
                            "updated LITKEY rate from CoinGecko"
                        );
                    }
                }
                RateValidation::RejectInvalid | RateValidation::RejectAbsurdJump => {
                    tracing::warn!(
                        candidate_usd_wei_per_litkey = %usd_wei,
                        recent_usd_wei_per_litkey = current.as_ref().map(|r| r.usd_wei_per_litkey.as_str()),
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
            usd_wei_per_litkey = rate.usd_wei_per_litkey,
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
    let usd_wei = req.usd_wei_per_litkey.trim();
    validate_manual_usd_wei(usd_wei).map_err(|e| err(Status::BadRequest, e.to_string()))?;
    set_manual(pool, usd_wei, operator.id)
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
    let effective_usd_wei_per_litkey = rate
        .as_ref()
        .filter(|_| !crediting_paused)
        .map(|r| apply_discount_to_usd_wei(&r.usd_wei_per_litkey, discount_basis_points))
        .transpose()
        .map_err(server_err)?;
    Ok(RateResponse {
        rate,
        discount_basis_points,
        effective_usd_wei_per_litkey,
        crediting_paused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, OffsetDateTime};

    #[test]
    fn parses_coingecko_usd_to_18_decimal_usd_wei_without_losing_sub_cent_prices() {
        assert_eq!(
            parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":0.006}}"#).unwrap(),
            "6000000000000000"
        );
        assert_eq!(
            parse_coingecko_usd_wei(
                r#"{"0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49":{"usd":0.00568693}}"#,
            )
            .unwrap(),
            "5686930000000000"
        );
        assert_eq!(
            parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":0.000000000000000001}}"#).unwrap(),
            "1"
        );
        assert_eq!(
            parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":0.1234567891234567894}}"#).unwrap(),
            "123456789123456789"
        );
    }

    #[test]
    fn computes_stripe_cents_from_litkey_wei_with_final_step_rounding() {
        let one_litkey_wei = "1000000000000000000";
        let usd_wei_per_litkey = "6000000000000000"; // $0.006/LITKEY

        assert_eq!(
            litkey_wei_to_credit_cents(one_litkey_wei, usd_wei_per_litkey, 0).unwrap(),
            1
        );
        assert_eq!(
            litkey_wei_to_credit_cents(
                "1000000000000000000000", // 1,000 LITKEY
                usd_wei_per_litkey,
                2_000,
            )
            .unwrap(),
            750
        );
    }

    #[test]
    fn rejects_missing_zero_negative_non_finite_and_absurd_rates() {
        assert!(parse_coingecko_usd_wei(r#"{}"#).is_err());
        assert!(parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":0}}"#).is_err());
        assert!(parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":-1}}"#).is_err());
        assert!(parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":1e309}}"#).is_err());
        assert!(parse_coingecko_usd_wei(r#"{"lit-protocol":{"usd":10001}}"#).is_err());
    }

    #[test]
    fn truncates_long_external_error_bodies_for_logs() {
        assert_eq!(truncate_for_log("abcdef", 3), "abc…");
        assert_eq!(truncate_for_log("abc", 3), "abc");
    }

    #[test]
    fn rejects_candidate_more_than_100x_recent_rate() {
        assert_eq!(
            validate_candidate_usd_wei("1001", Some("10")),
            RateValidation::RejectAbsurdJump
        );
        assert_eq!(
            validate_candidate_usd_wei("1000", Some("10")),
            RateValidation::Accept
        );
        assert_eq!(
            validate_candidate_usd_wei("50", None),
            RateValidation::Accept
        );
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
            usd_wei_per_litkey: "12".to_string(),
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
            usd_wei_per_litkey: "100".to_string(),
            source: "coingecko".to_string(),
            fetched_at: now - Duration::hours(2),
            updated_by_operator_id: None,
            stale: true,
        };

        let missing = build_rate_response(None, 2_000).unwrap();
        assert!(missing.crediting_paused);
        assert_eq!(missing.effective_usd_wei_per_litkey, None);

        let stale = build_rate_response(Some(stale_rate), 2_000).unwrap();
        assert!(stale.crediting_paused);
        assert_eq!(stale.effective_usd_wei_per_litkey, None);
    }

    #[test]
    fn fresh_rate_exposes_discount_adjusted_effective_rate() {
        let now = OffsetDateTime::now_utc();
        let fresh_rate = LitkeyRate {
            usd_wei_per_litkey: "100".to_string(),
            source: "coingecko".to_string(),
            fetched_at: now,
            updated_by_operator_id: None,
            stale: false,
        };

        let response = build_rate_response(Some(fresh_rate), 2_000).unwrap();
        assert!(!response.crediting_paused);
        assert_eq!(
            response.effective_usd_wei_per_litkey,
            Some("125".to_string())
        );
    }

    #[test]
    fn manual_override_validation_accepts_positive_reasonable_usd_wei_only() {
        assert!(validate_manual_usd_wei("1").is_ok());
        assert!(validate_manual_usd_wei(MAX_USD_WEI_PER_LITKEY).is_ok());
        assert!(validate_manual_usd_wei("0").is_err());
        assert!(validate_manual_usd_wei("10000000000000000000001").is_err());
    }

    #[test]
    fn applies_litkey_discount_as_extra_credit_per_token() {
        assert_eq!(apply_discount_to_usd_wei("100", 0).unwrap(), "100");
        // 20% off means paying $0.80 worth of LITKEY buys $1.00 credit,
        // so each token credits 1 / (1 - 0.20) = 1.25x spot value.
        assert_eq!(apply_discount_to_usd_wei("100", 2_000).unwrap(), "125");
        assert_eq!(apply_discount_to_usd_wei("33", 2_000).unwrap(), "41");
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
