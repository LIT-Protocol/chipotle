use anyhow::Result;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub seq: i64,
    pub role: String,
    pub ciphertext: Vec<u8>,
    pub enc_usage_meta: Option<Vec<u8>>,
    pub created_at: OffsetDateTime,
}

/// Append with a serialized seq: computed inside the insert so concurrent
/// appends to one conversation cannot collide (UNIQUE(conversation_id, seq)
/// backstops it; the caller retries on conflict).
pub async fn append(
    pool: &PgPool,
    id: Uuid,
    conversation_id: Uuid,
    seq: i64,
    role: &str,
    ciphertext: &[u8],
    enc_usage_meta: Option<&[u8]>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, seq, role, ciphertext, enc_usage_meta)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(conversation_id)
    .bind(seq)
    .bind(role)
    .bind(ciphertext)
    .bind(enc_usage_meta)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn next_seq(pool: &PgPool, conversation_id: Uuid) -> Result<i64> {
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT max(seq) FROM messages WHERE conversation_id = $1")
            .bind(conversation_id)
            .fetch_one(pool)
            .await?;
    Ok(row.0.unwrap_or(0) + 1)
}

pub async fn list(pool: &PgPool, conversation_id: Uuid) -> Result<Vec<MessageRow>> {
    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT id, conversation_id, seq, role, ciphertext, enc_usage_meta, created_at
         FROM messages WHERE conversation_id = $1 ORDER BY seq ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete the trailing assistant message (regenerate). Returns its seq.
pub async fn pop_trailing_assistant(pool: &PgPool, conversation_id: Uuid) -> Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "DELETE FROM messages WHERE conversation_id = $1
           AND seq = (SELECT max(seq) FROM messages WHERE conversation_id = $1)
           AND role = 'assistant'
         RETURNING seq",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(s,)| s))
}
