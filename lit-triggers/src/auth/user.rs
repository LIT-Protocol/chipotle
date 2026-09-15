//! `User` model + users-table queries.

use anyhow::Result;
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
    let row = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, email FROM users WHERE lower(email) = lower($1) LIMIT 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, email)| User { id, email }))
}

/// Look up only the user's `id` by email, avoiding materializing the email
/// (PII) into memory. Used by callers that just need a non-PII log identifier.
pub async fn find_id_by_email(pool: &PgPool, email: &str) -> Result<Option<Uuid>> {
    let row =
        sqlx::query_as::<_, (Uuid,)>("SELECT id FROM users WHERE lower(email) = lower($1) LIMIT 1")
            .bind(email)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(id,)| id))
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>> {
    let row = sqlx::query_as::<_, (Uuid, String)>("SELECT id, email FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|(id, email)| User { id, email }))
}

pub async fn find_or_create_by_email(pool: &PgPool, email: &str) -> Result<User> {
    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, (Uuid, String)>(
        "INSERT INTO users (id, email) VALUES ($1, $2)
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
         RETURNING id, email",
    )
    .bind(id)
    .bind(email)
    .fetch_one(pool)
    .await?;

    Ok(User {
        id: row.0,
        email: row.1,
    })
}

pub async fn touch_last_login(pool: &PgPool, id: Uuid) {
    let now = OffsetDateTime::now_utc();
    if let Err(e) = sqlx::query("UPDATE users SET last_login_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
    {
        tracing::warn!("users::touch_last_login({id}) failed: {e}");
    }
}
