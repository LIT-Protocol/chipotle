use anyhow::{anyhow, Result};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: Uuid,
    pub user_ref_hash: String,
    pub wrapped_dek: Vec<u8>,
    pub enc_title: Option<Vec<u8>>,
    pub model_id: String,
    pub version: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

const COLS: &str =
    "id, user_ref_hash, wrapped_dek, enc_title, model_id, version, created_at, updated_at";

pub async fn create(
    pool: &PgPool,
    id: Uuid,
    user_ref_hash: &str,
    wrapped_dek: &[u8],
    model_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO conversations (id, user_ref_hash, wrapped_dek, model_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(user_ref_hash)
    .bind(wrapped_dek)
    .bind(model_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_for_user(pool: &PgPool, user_ref_hash: &str) -> Result<Vec<ConversationRow>> {
    let rows = sqlx::query_as::<_, ConversationRow>(&format!(
        "SELECT {COLS} FROM conversations WHERE user_ref_hash = $1 ORDER BY updated_at DESC"
    ))
    .bind(user_ref_hash)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Owner-scoped fetch: the query itself enforces tenancy.
pub async fn get_owned(
    pool: &PgPool,
    id: Uuid,
    user_ref_hash: &str,
) -> Result<Option<ConversationRow>> {
    let row = sqlx::query_as::<_, ConversationRow>(&format!(
        "SELECT {COLS} FROM conversations WHERE id = $1 AND user_ref_hash = $2"
    ))
    .bind(id)
    .bind(user_ref_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Optimistic concurrency: bump version iff the caller saw the latest
/// (CPL-364's expected_version -> 409 shape). Returns the new version, or
/// None on conflict.
pub async fn set_title(
    pool: &PgPool,
    id: Uuid,
    user_ref_hash: &str,
    enc_title: &[u8],
    expected_version: i64,
) -> Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "UPDATE conversations SET enc_title = $1, version = version + 1, updated_at = now()
         WHERE id = $2 AND user_ref_hash = $3 AND version = $4
         RETURNING version",
    )
    .bind(enc_title)
    .bind(id)
    .bind(user_ref_hash)
    .bind(expected_version)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(v,)| v))
}

/// Title set by the server itself (auto-title): no version expectation.
pub async fn set_title_unchecked(
    pool: &PgPool,
    id: Uuid,
    user_ref_hash: &str,
    enc_title: &[u8],
) -> Result<()> {
    sqlx::query(
        "UPDATE conversations SET enc_title = $1, version = version + 1, updated_at = now()
         WHERE id = $2 AND user_ref_hash = $3",
    )
    .bind(enc_title)
    .bind(id)
    .bind(user_ref_hash)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn touch(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Hard delete; messages cascade.
pub async fn delete(pool: &PgPool, id: Uuid, user_ref_hash: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM conversations WHERE id = $1 AND user_ref_hash = $2")
        .bind(id)
        .bind(user_ref_hash)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Anon -> account migration (section 5.4): within one transaction, rewrap
/// every conversation DEK from the anon KEK to the account KEK and repoint
/// rows to the new user_ref_hash. O(conversations), not O(messages).
/// The rewrap closure runs in-enclave; this function only moves bytes.
pub async fn migrate_owner(
    pool: &PgPool,
    old_hash: &str,
    new_hash: &str,
    rewrap: impl Fn(Uuid, &[u8]) -> Result<Vec<u8>>,
) -> Result<u64> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO chat_users (user_ref_hash, kind) VALUES ($1, 'account')
         ON CONFLICT (user_ref_hash) DO NOTHING",
    )
    .bind(new_hash)
    .execute(&mut *tx)
    .await?;
    let rows: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
        "SELECT id, wrapped_dek FROM conversations WHERE user_ref_hash = $1 FOR UPDATE",
    )
    .bind(old_hash)
    .fetch_all(&mut *tx)
    .await?;
    let count = rows.len() as u64;
    for (id, wrapped) in rows {
        let rewrapped = rewrap(id, &wrapped)
            .map_err(|e| anyhow!("rewrap failed for conversation {id}: {e}"))?;
        sqlx::query(
            "UPDATE conversations SET user_ref_hash = $1, wrapped_dek = $2, version = version + 1
             WHERE id = $3",
        )
        .bind(new_hash)
        .bind(rewrapped)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    // Retire the anon identity. The meter (if any) dies with it — budgets
    // restart on the account, which also starts the paid-tier ledger clean.
    sqlx::query("DELETE FROM chat_users WHERE user_ref_hash = $1")
        .bind(old_hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(count)
}
