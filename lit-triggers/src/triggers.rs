//! Session-protected trigger CRUD API.

use rocket::http::Status;
use rocket::response::status::Custom;
use rocket::serde::json::Json;
use rocket::{delete, get, patch, post, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::User;
use crate::config::Config;
use crate::crypto;
use ipfs_hasher::IpfsHasher;

type ApiResult<T> = Result<Json<T>, Custom<Json<ErrorResponse>>>;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: &'static str,
}

fn err(status: Status, error: &'static str) -> Custom<Json<ErrorResponse>> {
    Custom(status, Json(ErrorResponse { error }))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Webhook,
    ChainEvent,
    Schedule,
}

impl TriggerKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::ChainEvent => "chain_event",
            Self::Schedule => "schedule",
        }
    }

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "webhook" => Ok(Self::Webhook),
            "chain_event" => Ok(Self::ChainEvent),
            "schedule" => Ok(Self::Schedule),
            _ => anyhow::bail!("invalid trigger kind"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub kind: TriggerKind,
    pub action_code: String,
    pub action_cid: String,
    pub default_params: Value,
    pub chipotle_account_address: Option<String>,
    pub max_runs_per_minute: Option<i32>,
    pub max_queued_runs: Option<i32>,
    pub config: Value,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateTriggerRequest {
    pub name: String,
    pub kind: TriggerKind,
    pub action_code: String,
    #[serde(default = "empty_object")]
    pub default_params: Value,
    pub usage_api_key: String,
    pub chipotle_account_address: Option<String>,
    pub max_runs_per_minute: Option<i32>,
    pub max_queued_runs: Option<i32>,
    #[serde(default = "empty_object")]
    pub config: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTriggerRequest {
    pub name: Option<String>,
    pub kind: Option<TriggerKind>,
    pub action_code: Option<String>,
    pub default_params: Option<Value>,
    pub usage_api_key: Option<String>,
    pub chipotle_account_address: Option<String>,
    pub max_runs_per_minute: Option<i32>,
    pub max_queued_runs: Option<i32>,
    pub config: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct TriggerRunResponse {
    pub id: Uuid,
    pub trigger_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub finished_at: Option<OffsetDateTime>,
    pub status: String,
    pub input: Option<Value>,
    pub response: Option<Value>,
    pub error: Option<String>,
    pub attempt: i32,
}

#[derive(Debug, Serialize)]
pub struct ListRunsResponse {
    pub runs: Vec<TriggerRunResponse>,
}

#[derive(Debug, Serialize)]
pub struct TestTriggerResponse {
    pub error: &'static str,
    pub message: &'static str,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

fn parse_id(id: &str) -> Result<Uuid, Custom<Json<ErrorResponse>>> {
    Uuid::parse_str(id).map_err(|_| err(Status::BadRequest, "invalid_id"))
}

#[post("/api/triggers", data = "<req>")]
pub async fn create_trigger(
    user: User,
    pool: &State<PgPool>,
    config: &State<Config>,
    req: Json<CreateTriggerRequest>,
) -> ApiResult<TriggerResponse> {
    let req = req.into_inner();
    validate_create(&req)?;
    let (nonce, ciphertext) =
        crypto::encrypt_usage_key(&config.usage_key_encryption_key, &req.usage_api_key)
            .map_err(|_| err(Status::InternalServerError, "encryption_failed"))?;
    let action_cid = cid_for_action_code(&req.action_code);

    let row = sqlx::query(
        "INSERT INTO triggers (
            id, user_id, name, kind, action_code, action_cid, default_params,
            usage_api_key_ciphertext, usage_api_key_nonce, chipotle_account_address,
            max_runs_per_minute, max_queued_runs, config
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
         RETURNING id, user_id, name, kind, action_code, action_cid, default_params,
                   chipotle_account_address, max_runs_per_minute, max_queued_runs,
                   config, enabled, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(user.id)
    .bind(req.name)
    .bind(req.kind.as_str())
    .bind(req.action_code)
    .bind(action_cid)
    .bind(req.default_params)
    .bind(ciphertext)
    .bind(nonce)
    .bind(req.chipotle_account_address)
    .bind(req.max_runs_per_minute)
    .bind(req.max_queued_runs)
    .bind(req.config)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| {
        tracing::warn!("create trigger failed: {e}");
        err(Status::InternalServerError, "create_failed")
    })?;

    row_to_trigger(row)
        .map(Json)
        .map_err(|_| err(Status::InternalServerError, "decode_failed"))
}

#[get("/api/triggers")]
pub async fn list_triggers(user: User, pool: &State<PgPool>) -> ApiResult<Vec<TriggerResponse>> {
    let rows = sqlx::query(
        "SELECT id, user_id, name, kind, action_code, action_cid, default_params,
                chipotle_account_address, max_runs_per_minute, max_queued_runs,
                config, enabled, created_at, updated_at
         FROM triggers WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|_| err(Status::InternalServerError, "list_failed"))?;

    let triggers = rows
        .into_iter()
        .map(row_to_trigger)
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(|_| err(Status::InternalServerError, "decode_failed"))?;
    Ok(Json(triggers))
}

#[get("/api/triggers/<id>")]
pub async fn get_trigger(user: User, pool: &State<PgPool>, id: &str) -> ApiResult<TriggerResponse> {
    let id = parse_id(id)?;
    let row = sqlx::query(
        "SELECT id, user_id, name, kind, action_code, action_cid, default_params,
                chipotle_account_address, max_runs_per_minute, max_queued_runs,
                config, enabled, created_at, updated_at
         FROM triggers WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|_| err(Status::InternalServerError, "get_failed"))?
    .ok_or_else(|| err(Status::NotFound, "not_found"))?;

    row_to_trigger(row)
        .map(Json)
        .map_err(|_| err(Status::InternalServerError, "decode_failed"))
}

#[patch("/api/triggers/<id>", data = "<req>")]
pub async fn update_trigger(
    user: User,
    pool: &State<PgPool>,
    config: &State<Config>,
    id: &str,
    req: Json<UpdateTriggerRequest>,
) -> ApiResult<TriggerResponse> {
    let id = parse_id(id)?;
    let existing = get_existing_for_update(pool.inner(), user.id, id).await?;
    let req = req.into_inner();
    let name = req.name.unwrap_or(existing.name);
    let kind = req.kind.unwrap_or(existing.kind);
    let action_code = req.action_code.unwrap_or(existing.action_code);
    let action_cid = cid_for_action_code(&action_code);
    let default_params = req.default_params.unwrap_or(existing.default_params);
    let config_json = req.config.unwrap_or(existing.config);
    validate_common(&name, &action_code, &default_params, &config_json)?;

    let (nonce, ciphertext) = match req.usage_api_key {
        Some(k) => {
            if k.trim().is_empty() {
                return Err(err(Status::BadRequest, "usage_api_key_required"));
            }
            crypto::encrypt_usage_key(&config.usage_key_encryption_key, &k)
                .map_err(|_| err(Status::InternalServerError, "encryption_failed"))?
        }
        None => (
            existing.usage_api_key_nonce,
            existing.usage_api_key_ciphertext,
        ),
    };

    let row = sqlx::query(
        "UPDATE triggers SET
            name = $3, kind = $4, action_code = $5, action_cid = $6,
            default_params = $7, usage_api_key_ciphertext = $8,
            usage_api_key_nonce = $9, chipotle_account_address = $10,
            max_runs_per_minute = $11, max_queued_runs = $12, config = $13,
            enabled = $14, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, user_id, name, kind, action_code, action_cid, default_params,
                   chipotle_account_address, max_runs_per_minute, max_queued_runs,
                   config, enabled, created_at, updated_at",
    )
    .bind(id)
    .bind(user.id)
    .bind(name)
    .bind(kind.as_str())
    .bind(action_code)
    .bind(action_cid)
    .bind(default_params)
    .bind(ciphertext)
    .bind(nonce)
    .bind(
        req.chipotle_account_address
            .or(existing.chipotle_account_address),
    )
    .bind(req.max_runs_per_minute.or(existing.max_runs_per_minute))
    .bind(req.max_queued_runs.or(existing.max_queued_runs))
    .bind(config_json)
    .bind(req.enabled.unwrap_or(existing.enabled))
    .fetch_one(pool.inner())
    .await
    .map_err(|_| err(Status::InternalServerError, "update_failed"))?;

    row_to_trigger(row)
        .map(Json)
        .map_err(|_| err(Status::InternalServerError, "decode_failed"))
}

#[delete("/api/triggers/<id>")]
pub async fn delete_trigger(user: User, pool: &State<PgPool>, id: &str) -> Status {
    let Ok(id) = Uuid::parse_str(id) else {
        return Status::BadRequest;
    };
    match sqlx::query("DELETE FROM triggers WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(pool.inner())
        .await
    {
        Ok(_) => Status::NoContent,
        Err(e) => {
            tracing::warn!("delete trigger failed: {e}");
            Status::InternalServerError
        }
    }
}

#[get("/api/triggers/<id>/runs?<limit>&<offset>")]
pub async fn list_runs(
    user: User,
    pool: &State<PgPool>,
    id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> ApiResult<ListRunsResponse> {
    let id = parse_id(id)?;
    ensure_owned(pool.inner(), user.id, id).await?;
    let limit = limit.unwrap_or(50).clamp(1, 100);
    let offset = offset.unwrap_or(0).max(0);
    let rows = sqlx::query(
        "SELECT id, trigger_id, started_at, finished_at, status, input, response, error, attempt
         FROM trigger_runs WHERE trigger_id = $1 ORDER BY started_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool.inner())
    .await
    .map_err(|_| err(Status::InternalServerError, "runs_failed"))?;

    let runs = rows
        .into_iter()
        .map(|row| TriggerRunResponse {
            id: row.get("id"),
            trigger_id: row.get("trigger_id"),
            started_at: row.get("started_at"),
            finished_at: row.get("finished_at"),
            status: row.get("status"),
            input: row.get("input"),
            response: row.get("response"),
            error: row.get("error"),
            attempt: row.get("attempt"),
        })
        .collect();
    Ok(Json(ListRunsResponse { runs }))
}

#[post("/api/triggers/<id>/test")]
pub async fn test_trigger(
    user: User,
    pool: &State<PgPool>,
    id: &str,
) -> Result<Custom<Json<TestTriggerResponse>>, Custom<Json<ErrorResponse>>> {
    let id = parse_id(id)?;
    ensure_owned(pool.inner(), user.id, id).await?;
    Ok(Custom(
        Status::NotImplemented,
        Json(TestTriggerResponse {
            error: "not_implemented",
            message: "Trigger test execution is planned for the worker phase.",
        }),
    ))
}

struct ExistingTrigger {
    name: String,
    kind: TriggerKind,
    action_code: String,
    default_params: Value,
    usage_api_key_ciphertext: Vec<u8>,
    usage_api_key_nonce: Vec<u8>,
    chipotle_account_address: Option<String>,
    max_runs_per_minute: Option<i32>,
    max_queued_runs: Option<i32>,
    config: Value,
    enabled: bool,
}

async fn get_existing_for_update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<ExistingTrigger, Custom<Json<ErrorResponse>>> {
    let row = sqlx::query(
        "SELECT name, kind, action_code, default_params,
                usage_api_key_ciphertext, usage_api_key_nonce, chipotle_account_address,
                max_runs_per_minute, max_queued_runs, config, enabled
         FROM triggers WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| err(Status::InternalServerError, "get_failed"))?
    .ok_or_else(|| err(Status::NotFound, "not_found"))?;

    Ok(ExistingTrigger {
        name: row.get("name"),
        kind: TriggerKind::from_str(row.get::<String, _>("kind").as_str())
            .map_err(|_| err(Status::InternalServerError, "decode_failed"))?,
        action_code: row.get("action_code"),
        default_params: row.get("default_params"),
        usage_api_key_ciphertext: row.get("usage_api_key_ciphertext"),
        usage_api_key_nonce: row.get("usage_api_key_nonce"),
        chipotle_account_address: row.get("chipotle_account_address"),
        max_runs_per_minute: row.get("max_runs_per_minute"),
        max_queued_runs: row.get("max_queued_runs"),
        config: row.get("config"),
        enabled: row.get("enabled"),
    })
}

async fn ensure_owned(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<(), Custom<Json<ErrorResponse>>> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM triggers WHERE id = $1 AND user_id = $2)",
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| err(Status::InternalServerError, "get_failed"))?;
    if exists {
        Ok(())
    } else {
        Err(err(Status::NotFound, "not_found"))
    }
}

fn row_to_trigger(row: sqlx::postgres::PgRow) -> anyhow::Result<TriggerResponse> {
    Ok(TriggerResponse {
        id: row.get("id"),
        user_id: row.get("user_id"),
        name: row.get("name"),
        kind: TriggerKind::from_str(row.get::<String, _>("kind").as_str())?,
        action_code: row.get("action_code"),
        action_cid: row.get("action_cid"),
        default_params: row.get("default_params"),
        chipotle_account_address: row.get("chipotle_account_address"),
        max_runs_per_minute: row.get("max_runs_per_minute"),
        max_queued_runs: row.get("max_queued_runs"),
        config: row.get("config"),
        enabled: row.get("enabled"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub fn cid_for_action_code(code: &str) -> String {
    IpfsHasher::default().compute(code.as_bytes())
}

fn validate_create(req: &CreateTriggerRequest) -> Result<(), Custom<Json<ErrorResponse>>> {
    if req.usage_api_key.trim().is_empty() {
        return Err(err(Status::BadRequest, "usage_api_key_required"));
    }
    validate_common(
        &req.name,
        &req.action_code,
        &req.default_params,
        &req.config,
    )
}

fn validate_common(
    name: &str,
    action_code: &str,
    default_params: &Value,
    config: &Value,
) -> Result<(), Custom<Json<ErrorResponse>>> {
    if name.trim().is_empty() {
        return Err(err(Status::BadRequest, "name_required"));
    }
    if action_code.trim().is_empty() {
        return Err(err(Status::BadRequest, "action_code_required"));
    }

    if !default_params.is_object() {
        return Err(err(Status::BadRequest, "default_params_must_be_object"));
    }
    if !config.is_object() {
        return Err(err(Status::BadRequest, "config_must_be_object"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_kind_serializes_as_api_values() {
        assert_eq!(
            serde_json::to_string(&TriggerKind::Webhook).unwrap(),
            "\"webhook\""
        );
        assert_eq!(
            serde_json::to_string(&TriggerKind::ChainEvent).unwrap(),
            "\"chain_event\""
        );
        assert_eq!(
            serde_json::to_string(&TriggerKind::Schedule).unwrap(),
            "\"schedule\""
        );
    }

    #[test]
    fn create_validation_rejects_secretless_request() {
        let req = CreateTriggerRequest {
            name: "n".into(),
            kind: TriggerKind::Webhook,
            action_code: "code".into(),
            default_params: empty_object(),
            usage_api_key: "".into(),
            chipotle_account_address: None,
            max_runs_per_minute: None,
            max_queued_runs: None,
            config: empty_object(),
        };
        assert!(validate_create(&req).is_err());
    }

    #[test]
    fn cid_for_action_code_is_deterministic() {
        let cid = cid_for_action_code("console.log('lit');");
        assert_eq!(cid, cid_for_action_code("console.log('lit');"));
        assert!(cid.starts_with("Qm"), "unexpected CIDv0: {cid}");
    }
}
