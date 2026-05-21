//! `Operator` model + operators-table queries.

use anyhow::Result;
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize)]
pub struct Operator {
    pub id: i64,
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Mod,
    Admin,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Mod => "mod",
            Role::Admin => "admin",
        }
    }

    fn from_db_str(s: &str) -> Result<Self> {
        match s {
            "mod" => Ok(Role::Mod),
            "admin" => Ok(Role::Admin),
            other => anyhow::bail!("unknown operator role in DB: {other}"),
        }
    }
}

/// Look up an operator by email (case-insensitive).
///
/// Returns `Ok(None)` if no operator with that email exists.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<Operator>> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, email, role FROM operators WHERE lower(email) = lower($1) LIMIT 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    row.map(|(id, email, role)| {
        Ok(Operator {
            id,
            email,
            role: Role::from_db_str(&role)?,
        })
    })
    .transpose()
}

/// Look up an operator by row id. Used by the session guard.
pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Operator>> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, email, role FROM operators WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(|(id, email, role)| {
        Ok(Operator {
            id,
            email,
            role: Role::from_db_str(&role)?,
        })
    })
    .transpose()
}

/// Update the operator's last_login_at to now. Best-effort; errors logged
/// but not propagated.
pub async fn touch_last_login(pool: &PgPool, id: i64) {
    let now = OffsetDateTime::now_utc();
    if let Err(e) = sqlx::query("UPDATE operators SET last_login_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
    {
        tracing::warn!("operators::touch_last_login({id}) failed: {e}");
    }
}
