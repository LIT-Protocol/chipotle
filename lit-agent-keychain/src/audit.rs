//! Per-tenant access log: every grant/reference decision, allow or deny.

use anyhow::Result;
use rocket::serde::json::Json;
use rocket::{get, State};
use serde::Serialize;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::agents::AgentKey;
use crate::api::{internal, ApiResult};
use crate::auth::User;
use crate::secrets::SecretRow;
use crate::tenants;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Grant,
    Reference,
}

impl Event {
    fn as_str(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Reference => "reference",
        }
    }
}

/// Identity of the secret an access decision was about. The name is stored
/// alongside the id so the row stays readable after the secret is deleted
/// (`secret_id` is intentionally not a foreign key).
#[derive(Debug, Clone, Copy)]
pub struct SecretRef<'a> {
    /// `None` when the agent asked for a name that doesn't exist.
    pub id: Option<Uuid>,
    pub name: &'a str,
}

pub async fn record(
    pool: &PgPool,
    tenant_id: Uuid,
    secret: SecretRef<'_>,
    agent: &AgentKey,
    event: Event,
    allowed: bool,
    reason: Option<&str>,
) {
    let r = sqlx::query(
        "INSERT INTO access_log (id, tenant_id, secret_id, secret_name, agent_id, agent_name, event, decision, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(secret.id)
    .bind(secret.name)
    .bind(agent.id)
    .bind(&agent.name)
    .bind(event.as_str())
    .bind(if allowed { "allow" } else { "deny" })
    .bind(reason)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::warn!("access_log insert failed: {e}");
    }
}

/// Mandatory allow-record insert used inside the grant-issuance transaction:
/// unlike [`record`], a failure here propagates so a grant is never handed out
/// without its audit/quota row.
pub async fn record_allow_tx(
    tx: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    secret: &SecretRow,
    agent: &AgentKey,
    event: Event,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO access_log (id, tenant_id, secret_id, secret_name, agent_id, agent_name, event, decision, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'allow', NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(secret.id)
    .bind(&secret.name)
    .bind(agent.id)
    .bind(&agent.name)
    .bind(event.as_str())
    .execute(tx)
    .await?;
    Ok(())
}

/// Stable per-secret advisory-lock key (first 8 bytes of the UUID). Used with
/// `pg_advisory_xact_lock` to serialize quota checks and version allocation.
pub fn secret_lock_key(secret_id: Uuid) -> i64 {
    let b = secret_id.as_bytes();
    i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Successful grants for a secret in the trailing 24 hours (policy rate window).
pub async fn grants_last_24h<'e, E: sqlx::PgExecutor<'e>>(ex: E, secret_id: Uuid) -> Result<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM access_log
         WHERE secret_id = $1 AND event = 'grant' AND decision = 'allow'
           AND created_at > now() - interval '24 hours'",
    )
    .bind(secret_id)
    .fetch_one(ex)
    .await?;
    Ok(n)
}

#[derive(Debug, Serialize)]
pub struct AccessLogEntry {
    pub id: Uuid,
    pub secret_id: Option<Uuid>,
    /// Name at the time of the event (survives deletion/rename of the secret).
    pub secret_name: Option<String>,
    /// True when the secret this row refers to no longer exists.
    pub secret_deleted: bool,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub event: String,
    pub decision: String,
    pub reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[get("/api/audit?<limit>")]
pub async fn list_audit(
    user: User,
    pool: &State<PgPool>,
    limit: Option<i64>,
) -> ApiResult<Vec<AccessLogEntry>> {
    let Some(tenant) = tenants::find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    else {
        return Ok(Json(vec![]));
    };
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    let rows = sqlx::query(
        "SELECT l.id, l.secret_id,
                COALESCE(l.secret_name, s.name) AS secret_name,
                (l.secret_id IS NOT NULL AND s.id IS NULL) AS secret_deleted,
                l.agent_id, COALESCE(l.agent_name, a.name) AS agent_name,
                l.event, l.decision, l.reason, l.created_at
         FROM access_log l
         LEFT JOIN secrets s ON s.id = l.secret_id
         LEFT JOIN agents a ON a.id = l.agent_id
         WHERE l.tenant_id = $1
         ORDER BY l.created_at DESC
         LIMIT $2",
    )
    .bind(tenant.id)
    .bind(limit)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal("audit_list_failed", e))?;

    Ok(Json(
        rows.into_iter()
            .map(|r| AccessLogEntry {
                id: r.get("id"),
                secret_id: r.get("secret_id"),
                secret_name: r.get("secret_name"),
                secret_deleted: r.get("secret_deleted"),
                agent_id: r.get("agent_id"),
                agent_name: r.get("agent_name"),
                event: r.get("event"),
                decision: r.get("decision"),
                reason: r.get("reason"),
                created_at: r.get("created_at"),
            })
            .collect(),
    ))
}
