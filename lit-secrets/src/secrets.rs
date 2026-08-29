//! Secret CRUD. Values are sealed to the tenant vault PKP by running the
//! encrypt action on Chipotle; only ciphertext is stored here.

use anyhow::Result;
use regex::Regex;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{delete, get, patch, post, put, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::LazyLock;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::actions::ActionSet;
use crate::api::{err, err_detail, internal, upstream, ApiError, ApiResult};
use crate::auth::User;
use crate::chipotle::{unwrap_response, ChipotleClient};
use crate::config::Config;
use crate::policy::{Policy, Release};
use crate::tenants::{self, ProvisionLock, Tenant};

static NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_.\-]{0,127}$").expect("valid regex"));

#[derive(Debug, Clone)]
pub struct SecretRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub kind: String,
    pub environment: String,
    pub release: Release,
    pub policy: Policy,
    pub current_version: i32,
    pub disabled: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct VersionRow {
    pub version: i32,
    pub ciphertext: String,
    pub ciphertext_hash: String,
    pub created_at: OffsetDateTime,
}

const SECRET_COLS: &str =
    "id, tenant_id, name, kind, environment, release, policy, current_version, disabled, created_at, updated_at";

fn row_to_secret(r: sqlx::postgres::PgRow) -> Result<SecretRow> {
    let release_raw: String = r.get("release");
    let release = Release::parse(&release_raw)
        .ok_or_else(|| anyhow::anyhow!("unknown release tier in db: {release_raw}"))?;
    let policy_json: Value = r.get("policy");
    let policy: Policy = serde_json::from_value(policy_json)?;
    Ok(SecretRow {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        name: r.get("name"),
        kind: r.get("kind"),
        environment: r.get("environment"),
        release,
        policy,
        current_version: r.get("current_version"),
        disabled: r.get("disabled"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

pub async fn find_secret(pool: &PgPool, tenant_id: Uuid, name: &str) -> Result<Option<SecretRow>> {
    let row = sqlx::query(&format!(
        "SELECT {SECRET_COLS} FROM secrets WHERE tenant_id = $1 AND name = $2"
    ))
    .bind(tenant_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    row.map(row_to_secret).transpose()
}

pub async fn find_version(
    pool: &PgPool,
    secret_id: Uuid,
    version: i32,
) -> Result<Option<VersionRow>> {
    let row = sqlx::query(
        "SELECT version, ciphertext, ciphertext_hash, created_at FROM secret_versions WHERE secret_id = $1 AND version = $2",
    )
    .bind(secret_id)
    .bind(version)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| VersionRow {
        version: r.get("version"),
        ciphertext: r.get("ciphertext"),
        ciphertext_hash: r.get("ciphertext_hash"),
        created_at: r.get("created_at"),
    }))
}

pub fn ciphertext_hash(ciphertext: &str) -> String {
    format!(
        "0x{}",
        hex::encode(alloy_primitives::keccak256(ciphertext.as_bytes()))
    )
}

/// Seal a value to the tenant vault by running the encrypt action.
async fn seal_value(
    cfg: &Config,
    chipotle: &ChipotleClient,
    actions: &ActionSet,
    tenant: &Tenant,
    value: &str,
) -> Result<(String, String), ApiError> {
    let service_key =
        tenants::service_key(cfg, tenant).map_err(|e| internal("service_key_decrypt_failed", e))?;
    let resp = chipotle
        .execute_lit_action(
            &service_key,
            &actions.encrypt_code,
            json!({ "pkpId": tenant.pkp_id, "value": value }),
        )
        .await
        .map_err(|e| upstream("chipotle_encrypt_failed", &e))?;
    let body = unwrap_response(&resp.response);
    let ciphertext = body
        .get("ciphertext")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!(logs = %resp.logs.chars().take(300).collect::<String>(), "encrypt action returned no ciphertext");
            err(Status::BadGateway, "chipotle_encrypt_malformed")
        })?
        .to_string();
    let hash = ciphertext_hash(&ciphertext);
    Ok((ciphertext, hash))
}

fn validate_name(name: &str) -> Result<(), ApiError> {
    if NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(err_detail(
            Status::BadRequest,
            "invalid_name",
            "name must be 1-128 chars of [A-Za-z0-9_.-] starting alphanumeric",
        ))
    }
}

fn validate_value(cfg: &Config, value: &str) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(err_detail(
            Status::BadRequest,
            "invalid_value",
            "value must not be empty",
        ));
    }
    if value.len() > cfg.max_secret_bytes {
        return Err(err_detail(
            Status::BadRequest,
            "value_too_large",
            format!("value exceeds {} bytes", cfg.max_secret_bytes),
        ));
    }
    Ok(())
}

fn validate_label(field: &str, v: &str) -> Result<(), ApiError> {
    if v.is_empty()
        || v.len() > 64
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(err_detail(
            Status::BadRequest,
            "invalid_label",
            format!("{field} must be 1-64 chars of [A-Za-z0-9_.-]"),
        ));
    }
    Ok(())
}

fn validate_policy(p: &Policy) -> Result<(), ApiError> {
    p.validate()
        .map_err(|m| err_detail(Status::BadRequest, "invalid_policy", m))
}

#[derive(Debug, Serialize)]
pub struct SecretResponse {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub environment: String,
    pub release: Release,
    pub policy: Policy,
    pub current_version: i32,
    pub disabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<SecretRow> for SecretResponse {
    fn from(s: SecretRow) -> Self {
        Self {
            id: s.id,
            name: s.name,
            kind: s.kind,
            environment: s.environment,
            release: s.release,
            policy: s.policy,
            current_version: s.current_version,
            disabled: s.disabled,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub version: i32,
    pub ciphertext_hash: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct SecretDetailResponse {
    #[serde(flatten)]
    pub secret: SecretResponse,
    pub versions: Vec<VersionResponse>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
    pub kind: Option<String>,
    pub environment: Option<String>,
    pub release: Option<Release>,
    pub policy: Option<Policy>,
}

#[derive(Debug, Deserialize)]
pub struct RotateSecretRequest {
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSecretRequest {
    pub kind: Option<String>,
    pub environment: Option<String>,
    pub release: Option<Release>,
    pub policy: Option<Policy>,
    pub disabled: Option<bool>,
}

#[post("/api/secrets", data = "<req>")]
pub async fn create_secret(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    actions: &State<ActionSet>,
    lock: &State<ProvisionLock>,
    req: Json<CreateSecretRequest>,
) -> ApiResult<SecretResponse> {
    let req = req.into_inner();
    let name = req.name.trim().to_string();
    validate_name(&name)?;
    validate_value(cfg, &req.value)?;
    let kind = req.kind.unwrap_or_else(|| "generic".into());
    let environment = req.environment.unwrap_or_else(|| "production".into());
    validate_label("kind", &kind)?;
    validate_label("environment", &environment)?;
    let release = req.release.unwrap_or(Release::Plaintext);
    let policy = req.policy.unwrap_or_default();
    validate_policy(&policy)?;

    let tenant = tenants::ensure_tenant(pool, cfg, chipotle, actions, lock, user.id).await?;
    if find_secret(pool, tenant.id, &name)
        .await
        .map_err(|e| internal("secret_lookup_failed", e))?
        .is_some()
    {
        return Err(err(Status::Conflict, "secret_exists"));
    }

    let (ciphertext, hash) = seal_value(cfg, chipotle, actions, &tenant, &req.value).await?;
    let policy_json =
        serde_json::to_value(&policy).map_err(|e| internal("policy_encode_failed", e))?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| internal("tx_begin_failed", e))?;
    let secret_id = Uuid::new_v4();
    let row = sqlx::query(&format!(
        "INSERT INTO secrets (id, tenant_id, name, kind, environment, release, policy, current_version)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 1)
         RETURNING {SECRET_COLS}"
    ))
    .bind(secret_id)
    .bind(tenant.id)
    .bind(&name)
    .bind(&kind)
    .bind(&environment)
    .bind(release.as_str())
    .bind(policy_json)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| internal("secret_insert_failed", e))?;
    sqlx::query(
        "INSERT INTO secret_versions (id, secret_id, version, ciphertext, ciphertext_hash) VALUES ($1, $2, 1, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(secret_id)
    .bind(&ciphertext)
    .bind(&hash)
    .execute(&mut *tx)
    .await
    .map_err(|e| internal("version_insert_failed", e))?;
    tx.commit()
        .await
        .map_err(|e| internal("tx_commit_failed", e))?;

    let secret = row_to_secret(row).map_err(|e| internal("secret_decode_failed", e))?;
    Ok(Json(secret.into()))
}

#[get("/api/secrets")]
pub async fn list_secrets(user: User, pool: &State<PgPool>) -> ApiResult<Vec<SecretResponse>> {
    let Some(tenant) = tenants::find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    else {
        return Ok(Json(vec![]));
    };
    let rows = sqlx::query(&format!(
        "SELECT {SECRET_COLS} FROM secrets WHERE tenant_id = $1 ORDER BY name"
    ))
    .bind(tenant.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal("secrets_list_failed", e))?;
    let out = rows
        .into_iter()
        .map(|r| row_to_secret(r).map(SecretResponse::from))
        .collect::<Result<Vec<_>>>()
        .map_err(|e| internal("secret_decode_failed", e))?;
    Ok(Json(out))
}

async fn load_owned(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
) -> Result<(Tenant, SecretRow), ApiError> {
    let tenant = tenants::find_by_user(pool, user_id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
        .ok_or_else(|| err(Status::NotFound, "not_found"))?;
    let secret = find_secret(pool, tenant.id, name)
        .await
        .map_err(|e| internal("secret_lookup_failed", e))?
        .ok_or_else(|| err(Status::NotFound, "not_found"))?;
    Ok((tenant, secret))
}

#[get("/api/secrets/<name>")]
pub async fn get_secret(
    user: User,
    pool: &State<PgPool>,
    name: &str,
) -> ApiResult<SecretDetailResponse> {
    let (_, secret) = load_owned(pool, user.id, name).await?;
    let rows = sqlx::query(
        "SELECT version, ciphertext_hash, created_at FROM secret_versions WHERE secret_id = $1 ORDER BY version DESC",
    )
    .bind(secret.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal("versions_list_failed", e))?;
    Ok(Json(SecretDetailResponse {
        secret: secret.into(),
        versions: rows
            .into_iter()
            .map(|r| VersionResponse {
                version: r.get("version"),
                ciphertext_hash: r.get("ciphertext_hash"),
                created_at: r.get("created_at"),
            })
            .collect(),
    }))
}

/// Rotate: seal a new value as the next version and make it current. Old
/// versions stay readable (by explicit version) until the secret is disabled.
#[put("/api/secrets/<name>", data = "<req>")]
pub async fn rotate_secret(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    actions: &State<ActionSet>,
    name: &str,
    req: Json<RotateSecretRequest>,
) -> ApiResult<SecretResponse> {
    validate_value(cfg, &req.value)?;
    let (tenant, secret) = load_owned(pool, user.id, name).await?;
    let (ciphertext, hash) = seal_value(cfg, chipotle, actions, &tenant, &req.value).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| internal("tx_begin_failed", e))?;
    let (next,): (i32,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM secret_versions WHERE secret_id = $1",
    )
    .bind(secret.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| internal("version_next_failed", e))?;
    sqlx::query(
        "INSERT INTO secret_versions (id, secret_id, version, ciphertext, ciphertext_hash) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(secret.id)
    .bind(next)
    .bind(&ciphertext)
    .bind(&hash)
    .execute(&mut *tx)
    .await
    .map_err(|e| internal("version_insert_failed", e))?;
    let row = sqlx::query(&format!(
        "UPDATE secrets SET current_version = $2, updated_at = now() WHERE id = $1 RETURNING {SECRET_COLS}"
    ))
    .bind(secret.id)
    .bind(next)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| internal("secret_update_failed", e))?;
    tx.commit()
        .await
        .map_err(|e| internal("tx_commit_failed", e))?;

    let secret = row_to_secret(row).map_err(|e| internal("secret_decode_failed", e))?;
    Ok(Json(secret.into()))
}

#[patch("/api/secrets/<name>", data = "<req>")]
pub async fn update_secret(
    user: User,
    pool: &State<PgPool>,
    name: &str,
    req: Json<UpdateSecretRequest>,
) -> ApiResult<SecretResponse> {
    let req = req.into_inner();
    let (_, secret) = load_owned(pool, user.id, name).await?;
    let kind = req.kind.unwrap_or(secret.kind);
    let environment = req.environment.unwrap_or(secret.environment);
    validate_label("kind", &kind)?;
    validate_label("environment", &environment)?;
    let release = req.release.unwrap_or(secret.release);
    let policy = req.policy.unwrap_or(secret.policy);
    validate_policy(&policy)?;
    let disabled = req.disabled.unwrap_or(secret.disabled);
    let policy_json =
        serde_json::to_value(&policy).map_err(|e| internal("policy_encode_failed", e))?;

    let row = sqlx::query(&format!(
        "UPDATE secrets SET kind = $2, environment = $3, release = $4, policy = $5, disabled = $6, updated_at = now()
         WHERE id = $1 RETURNING {SECRET_COLS}"
    ))
    .bind(secret.id)
    .bind(&kind)
    .bind(&environment)
    .bind(release.as_str())
    .bind(policy_json)
    .bind(disabled)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| internal("secret_update_failed", e))?;
    let secret = row_to_secret(row).map_err(|e| internal("secret_decode_failed", e))?;
    Ok(Json(secret.into()))
}

/// Delete = hard delete of the secret and its versions. The ciphertexts are
/// gone from our side; the vault PKP stays (other secrets share it).
#[delete("/api/secrets/<name>")]
pub async fn delete_secret(
    user: User,
    pool: &State<PgPool>,
    name: &str,
) -> Result<Status, ApiError> {
    let (_, secret) = load_owned(pool, user.id, name).await?;
    sqlx::query("DELETE FROM secrets WHERE id = $1")
        .bind(secret.id)
        .execute(pool.inner())
        .await
        .map_err(|e| internal("secret_delete_failed", e))?;
    Ok(Status::NoContent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rules() {
        assert!(NAME_RE.is_match("OPENAI_API_KEY"));
        assert!(NAME_RE.is_match("stripe.live-key"));
        assert!(!NAME_RE.is_match("_leading"));
        assert!(!NAME_RE.is_match("has space"));
        assert!(!NAME_RE.is_match(""));
        assert!(!NAME_RE.is_match(&"a".repeat(129)));
    }

    #[test]
    fn ciphertext_hash_matches_ethers_keccak_of_utf8() {
        // keccak256("abc") — well-known vector.
        assert_eq!(
            ciphertext_hash("abc"),
            "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }
}
