//! Public webhook endpoint for `kind = webhook` triggers.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use rocket::data::{Data, ToByteUnit};
use rocket::http::{ContentType, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::status::Custom;
use rocket::serde::json::Json;
use rocket::{post, State};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::config::Config;
use crate::triggers::ErrorResponse;
use crate::webhook_rate_limit::{RateLimitDecision, WebhookRateLimiter};

const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "proxy-authorization",
];

/// Non-secret verification headers that webhook senders attach so the receiver
/// can authenticate the request (HMAC signatures, event type/delivery ids).
/// These are safe to expose to the action — they are the receiver's to verify,
/// and require a shared secret (not present in the header) to forge.
const VERIFICATION_HEADERS: &[&str] = &[
    "x-hub-signature-256",
    "x-hub-signature",
    "x-github-event",
    "x-github-delivery",
    "x-github-hook-id",
    "stripe-signature",
    "x-slack-signature",
    "x-slack-request-timestamp",
    "x-webhook-signature",
    "x-signature",
];

#[derive(Debug, Serialize)]
pub struct WebhookAcceptedResponse {
    pub run_id: Uuid,
    pub status: &'static str,
}

type WebhookResult<T> = Result<Custom<Json<T>>, Custom<Json<ErrorResponse>>>;

#[post("/webhook/<trigger_id>", data = "<data>")]
pub async fn receive_webhook(
    trigger_id: &str,
    data: Data<'_>,
    content_type: Option<&ContentType>,
    meta: IncomingWebhookMeta,
    pool: &State<PgPool>,
    config: &State<Config>,
    limiter: &State<WebhookRateLimiter>,
) -> WebhookResult<WebhookAcceptedResponse> {
    let trigger_id =
        Uuid::parse_str(trigger_id).map_err(|_| err(Status::BadRequest, "invalid_id"))?;
    let ip = meta.ip.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    check_rate_limit(
        limiter.inner(),
        format!("ip:{ip}"),
        config.webhook_ip_max_requests_per_minute,
        "ip_rate_limited",
    )
    .await?;

    let trigger = load_webhook_trigger(pool.inner(), trigger_id).await?;

    check_rate_limit(
        limiter.inner(),
        format!("user:{}", trigger.user_id),
        config.webhook_user_max_requests_per_minute,
        "user_rate_limited",
    )
    .await?;
    check_rate_limit(
        limiter.inner(),
        format!("trigger:{trigger_id}"),
        trigger
            .max_runs_per_minute
            .unwrap_or(config.webhook_trigger_max_requests_per_minute as i32)
            .max(0) as u32,
        "trigger_rate_limited",
    )
    .await?;

    let input = build_run_input(
        data,
        content_type,
        &meta.headers,
        config.webhook_max_body_bytes as u64,
    )
    .await?;

    let run_id = enqueue_run_with_depth_check(
        pool.inner(),
        trigger_id,
        input,
        trigger.max_queued_runs,
        config,
    )
    .await?;

    Ok(Custom(
        Status::Accepted,
        Json(WebhookAcceptedResponse {
            run_id,
            status: "queued",
        }),
    ))
}

#[derive(Debug)]
struct WebhookTrigger {
    user_id: Uuid,
    max_runs_per_minute: Option<i32>,
    max_queued_runs: Option<i32>,
}

pub struct IncomingWebhookMeta {
    ip: Option<IpAddr>,
    headers: Vec<(String, String)>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for IncomingWebhookMeta {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(Self {
            ip: request.client_ip(),
            headers: request
                .headers()
                .iter()
                .map(|header| {
                    (
                        header.name().as_str().to_ascii_lowercase(),
                        header.value().to_string(),
                    )
                })
                .collect(),
        })
    }
}

async fn load_webhook_trigger(
    pool: &PgPool,
    trigger_id: Uuid,
) -> Result<WebhookTrigger, Custom<Json<ErrorResponse>>> {
    let row = sqlx::query(
        "SELECT user_id, kind, enabled, max_runs_per_minute, max_queued_runs
         FROM triggers WHERE id = $1",
    )
    .bind(trigger_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| err(Status::InternalServerError, "trigger_lookup_failed"))?
    .ok_or_else(|| err(Status::NotFound, "not_found"))?;

    let kind: String = row.get("kind");
    if kind != "webhook" {
        return Err(err(Status::NotFound, "not_found"));
    }
    let enabled: bool = row.get("enabled");
    if !enabled {
        return Err(err(Status::Gone, "trigger_disabled"));
    }

    Ok(WebhookTrigger {
        user_id: row.get("user_id"),
        max_runs_per_minute: row.get("max_runs_per_minute"),
        max_queued_runs: row.get("max_queued_runs"),
    })
}

async fn check_rate_limit(
    limiter: &WebhookRateLimiter,
    key: String,
    limit: u32,
    error: &'static str,
) -> Result<(), Custom<Json<ErrorResponse>>> {
    match limiter.check_and_record(key, limit).await {
        RateLimitDecision::Allowed => Ok(()),
        RateLimitDecision::Limited => Err(err(Status::TooManyRequests, error)),
    }
}

async fn enqueue_run_with_depth_check(
    pool: &PgPool,
    trigger_id: Uuid,
    input: Value,
    trigger_limit: Option<i32>,
    config: &Config,
) -> Result<Uuid, Custom<Json<ErrorResponse>>> {
    let max_queued = trigger_limit
        .unwrap_or(config.webhook_default_max_queued_runs as i32)
        .max(0) as i64;
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| err(Status::InternalServerError, "enqueue_failed"))?;

    // Serialize admission per trigger so concurrent webhooks cannot all observe
    // the same queue depth and overrun the configured cap.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(trigger_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|_| err(Status::InternalServerError, "queue_depth_failed"))?;

    let depth = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trigger_runs
         WHERE trigger_id = $1 AND status IN ('queued','running','retrying')",
    )
    .bind(trigger_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| err(Status::InternalServerError, "queue_depth_failed"))?;

    if depth >= max_queued {
        return Err(err(Status::TooManyRequests, "queue_depth_exceeded"));
    }

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO trigger_runs (id, trigger_id, status, input, attempt)
         VALUES ($1, $2, 'queued', $3, 1)",
    )
    .bind(run_id)
    .bind(trigger_id)
    .bind(input)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::warn!(trigger_id = %trigger_id, "webhook enqueue failed: {e}");
        err(Status::InternalServerError, "enqueue_failed")
    })?;

    tx.commit()
        .await
        .map_err(|_| err(Status::InternalServerError, "enqueue_failed"))?;
    Ok(run_id)
}

async fn build_run_input(
    data: Data<'_>,
    content_type: Option<&ContentType>,
    headers: &[(String, String)],
    max_body_bytes: u64,
) -> Result<Value, Custom<Json<ErrorResponse>>> {
    let bytes = data
        .open((max_body_bytes + 1).bytes())
        .into_bytes()
        .await
        .map_err(|_| err(Status::BadRequest, "body_read_failed"))?;

    if !bytes.is_complete() || bytes.value.len() as u64 > max_body_bytes {
        return Err(err(Status::PayloadTooLarge, "body_too_large"));
    }

    // Preserve the exact raw body so actions can verify signatures (GitHub
    // X-Hub-Signature-256, Stripe Stripe-Signature, etc.) that are computed as
    // an HMAC over the original bytes. parse_event borrows the bytes, so this
    // lossy copy is independent of the parsed `event`.
    let event_raw = String::from_utf8_lossy(bytes.value.as_ref()).into_owned();
    let event = parse_event(bytes.value.as_ref(), content_type)?;
    Ok(json!({
        "source": "webhook",
        "event": event,
        "event_raw": event_raw,
        "headers": safe_headers(headers),
    }))
}

fn parse_event(
    body: &[u8],
    content_type: Option<&ContentType>,
) -> Result<Value, Custom<Json<ErrorResponse>>> {
    let is_json = content_type
        .map(|ct| {
            ct.is_json()
                || ct.media_type().top() == "application" && ct.media_type().sub() == "json"
        })
        .unwrap_or(false);

    if is_json {
        serde_json::from_slice(body).map_err(|_| err(Status::BadRequest, "invalid_json"))
    } else {
        Ok(Value::String(String::from_utf8_lossy(body).into_owned()))
    }
}

fn safe_headers(headers: &[(String, String)]) -> Value {
    let mut out = Map::new();
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers.iter() {
        if SENSITIVE_HEADERS.contains(&name.as_str()) {
            continue;
        }
        if name.starts_with("x-forwarded")
            || name == "content-type"
            || name == "user-agent"
            || name == "x-request-id"
            || VERIFICATION_HEADERS.contains(&name.as_str())
        {
            grouped
                .entry(name.to_string())
                .or_default()
                .push(value.to_string());
        }
    }
    for (name, values) in grouped {
        out.insert(
            name,
            Value::Array(values.into_iter().map(Value::String).collect()),
        );
    }
    Value::Object(out)
}

fn err(status: Status, error: &'static str) -> Custom<Json<ErrorResponse>> {
    Custom(status, Json(ErrorResponse { error }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_event_when_content_type_is_json() {
        let event = parse_event(br#"{"hello":"world"}"#, Some(&ContentType::JSON)).unwrap();
        assert_eq!(event, json!({ "hello": "world" }));
    }

    #[test]
    fn parses_raw_event_as_string() {
        let event = parse_event(b"hello", Some(&ContentType::Plain)).unwrap();
        assert_eq!(event, json!("hello"));
    }

    #[test]
    fn safe_headers_excludes_credentials() {
        let headers = vec![
            ("authorization".to_string(), "Bearer secret".to_string()),
            ("cookie".to_string(), "a=b".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
            ("user-agent".to_string(), "test".to_string()),
        ];

        let safe = safe_headers(&headers);
        assert_eq!(
            safe,
            json!({
                "content-type": ["application/json"],
                "user-agent": ["test"]
            })
        );
    }

    #[test]
    fn safe_headers_passes_verification_headers() {
        let headers = vec![
            (
                "x-hub-signature-256".to_string(),
                "sha256=abc123".to_string(),
            ),
            ("x-github-event".to_string(), "release".to_string()),
            ("authorization".to_string(), "Bearer secret".to_string()),
        ];

        let safe = safe_headers(&headers);
        assert_eq!(
            safe,
            json!({
                "x-hub-signature-256": ["sha256=abc123"],
                "x-github-event": ["release"]
            })
        );
    }
}
