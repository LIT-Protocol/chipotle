//! Per-tenant access log: every grant/reference decision, allow or deny.

use anyhow::Result;
use rocket::serde::json::Json;
use rocket::{get, State};
use serde::Serialize;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::api::{internal, ApiResult};
use crate::auth::User;
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

pub async fn record(
    pool: &PgPool,
    tenant_id: Uuid,
    secret_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    event: Event,
    allowed: bool,
    reason: Option<&str>,
) {
    let r = sqlx::query(
        "INSERT INTO access_log (id, tenant_id, secret_id, agent_id, event, decision, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(secret_id)
    .bind(agent_id)
    .bind(event.as_str())
    .bind(if allowed { "allow" } else { "deny" })
    .bind(reason)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::warn!("access_log insert failed: {e}");
    }
}

/// Successful grants for a secret in the trailing 24 hours (policy rate window).
pub async fn grants_last_24h(pool: &PgPool, secret_id: Uuid) -> Result<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM access_log
         WHERE secret_id = $1 AND event = 'grant' AND decision = 'allow'
           AND created_at > now() - interval '24 hours'",
    )
    .bind(secret_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

#[derive(Debug, Serialize)]
pub struct AccessLogEntry {
    pub id: Uuid,
    pub secret_id: Option<Uuid>,
    pub secret_name: Option<String>,
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
        "SELECT l.id, l.secret_id, s.name AS secret_name, l.agent_id, a.name AS agent_name,
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
