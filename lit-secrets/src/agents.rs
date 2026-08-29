//! Agents: scoped Chipotle usage API keys minted per tenant. The same key
//! authenticates the agent to this control plane (bearer) *and* to Chipotle
//! (to run the reader action), so an agent carries exactly one credential.

use anyhow::Result;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use rocket::{delete, get, post, State};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::actions::ActionSet;
use crate::api::{err, err_detail, internal, upstream, ApiError, ApiResult};
use crate::auth::token::token_hash;
use crate::auth::User;
use crate::chipotle::ChipotleClient;
use crate::config::Config;
use crate::crypto;
use crate::tenants::{self, ProvisionLock};

/// Request guard: an agent authenticating with its Chipotle usage API key.
#[derive(Debug, Clone)]
pub struct AgentKey {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AgentKey {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(pool) = req.rocket().state::<PgPool>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        let token = req
            .headers()
            .get_one("authorization")
            .and_then(|v| {
                let v = v.trim();
                v.strip_prefix("Bearer ")
                    .or_else(|| v.strip_prefix("bearer "))
            })
            .map(str::trim)
            .or_else(|| req.headers().get_one("x-api-key").map(str::trim))
            .filter(|t| !t.is_empty() && t.len() <= 512);
        let Some(token) = token else {
            return Outcome::Error((Status::Unauthorized, ()));
        };
        match lookup(pool, token).await {
            Ok(Some(agent)) => Outcome::Success(agent),
            Ok(None) => Outcome::Error((Status::Unauthorized, ())),
            Err(e) => {
                tracing::warn!("agent key lookup failed: {e}");
                Outcome::Error((Status::InternalServerError, ()))
            }
        }
    }
}

async fn lookup(pool: &PgPool, usage_key: &str) -> Result<Option<AgentKey>> {
    let row = sqlx::query(
        "UPDATE agents SET last_seen_at = now()
         WHERE usage_key_hash = $1 AND revoked_at IS NULL
         RETURNING id, tenant_id, name",
    )
    .bind(token_hash(usage_key))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| AgentKey {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        name: r.get("name"),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct CreatedAgentResponse {
    #[serde(flatten)]
    pub agent: AgentResponse,
    /// Shown exactly once. Works as the bearer for this API and as the
    /// Chipotle usage key for running the reader action.
    pub usage_api_key: String,
    pub chipotle_api_base_url: String,
}

fn row_to_agent(r: sqlx::postgres::PgRow) -> AgentResponse {
    AgentResponse {
        id: r.get("id"),
        name: r.get("name"),
        created_at: r.get("created_at"),
        last_seen_at: r.get("last_seen_at"),
        revoked_at: r.get("revoked_at"),
    }
}

#[post("/api/agents", data = "<req>")]
pub async fn create_agent(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    actions: &State<ActionSet>,
    lock: &State<ProvisionLock>,
    req: Json<CreateAgentRequest>,
) -> ApiResult<CreatedAgentResponse> {
    let name: String = req.name.trim().chars().take(128).collect();
    if name.is_empty() {
        return Err(err_detail(
            Status::BadRequest,
            "invalid_name",
            "name is required",
        ));
    }
    let tenant = tenants::ensure_tenant(pool, cfg, chipotle, actions, lock, user.id).await?;

    let usage_key = chipotle
        .add_usage_api_key(
            &cfg.chipotle_master_api_key,
            &format!("lit-secrets agent: {name}"),
            &format!("Agent key for tenant {} (lit-secrets)", tenant.id),
            &[tenant.group_id_u64()],
        )
        .await
        .map_err(|e| upstream("chipotle_add_usage_api_key_failed", &e))?;
    let (nonce, ciphertext) = crypto::encrypt_usage_key(&cfg.usage_key_encryption_key, &usage_key)
        .map_err(|e| internal("agent_key_encrypt_failed", e))?;

    let row = sqlx::query(
        "INSERT INTO agents (id, tenant_id, name, usage_key_hash, usage_key_ciphertext, usage_key_nonce)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, name, created_at, last_seen_at, revoked_at",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.id)
    .bind(&name)
    .bind(token_hash(&usage_key))
    .bind(ciphertext)
    .bind(nonce)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| internal("agent_insert_failed", e))?;

    Ok(Json(CreatedAgentResponse {
        agent: row_to_agent(row),
        usage_api_key: usage_key,
        chipotle_api_base_url: chipotle.base_url().to_string(),
    }))
}

#[get("/api/agents")]
pub async fn list_agents(user: User, pool: &State<PgPool>) -> ApiResult<Vec<AgentResponse>> {
    let Some(tenant) = tenants::find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    else {
        return Ok(Json(vec![]));
    };
    let rows = sqlx::query(
        "SELECT id, name, created_at, last_seen_at, revoked_at FROM agents
         WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal("agents_list_failed", e))?;
    Ok(Json(rows.into_iter().map(row_to_agent).collect()))
}

/// Revoke: removes the usage key on Chipotle (so the reader action can no
/// longer be invoked with it) and marks the agent revoked here.
#[delete("/api/agents/<id>")]
pub async fn revoke_agent(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    id: &str,
) -> Result<Status, ApiError> {
    let id = Uuid::parse_str(id).map_err(|_| err(Status::BadRequest, "invalid_id"))?;
    let Some(tenant) = tenants::find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    let Some(row) = sqlx::query(
        "SELECT usage_key_ciphertext, usage_key_nonce, revoked_at FROM agents WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant.id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| internal("agent_lookup_failed", e))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    let already: Option<OffsetDateTime> = row.get("revoked_at");
    if already.is_some() {
        return Ok(Status::NoContent);
    }
    let ct: Vec<u8> = row.get("usage_key_ciphertext");
    let nonce: Vec<u8> = row.get("usage_key_nonce");
    let usage_key = crypto::decrypt_usage_key(&cfg.usage_key_encryption_key, &nonce, &ct)
        .map_err(|e| internal("agent_key_decrypt_failed", e))?;

    chipotle
        .remove_usage_api_key(&cfg.chipotle_master_api_key, &usage_key)
        .await
        .map_err(|e| upstream("chipotle_remove_usage_api_key_failed", &e))?;

    sqlx::query("UPDATE agents SET revoked_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool.inner())
        .await
        .map_err(|e| internal("agent_revoke_failed", e))?;
    Ok(Status::NoContent)
}
