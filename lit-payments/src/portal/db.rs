//! Postgres queries for the grants table.

use anyhow::Result;
use sqlx::{PgExecutor, PgPool};
use time::OffsetDateTime;

use super::types::GrantRow;

#[derive(Debug)]
pub struct NewGrant<'a> {
    pub operator_id: i64,
    pub stripe_customer_id: &'a str,
    pub wallet_address: &'a str,
    pub email: Option<&'a str>,
    pub cents: i64,
    pub note: &'a str,
    pub stripe_balance_transaction_id: &'a str,
    pub idempotency_key: &'a str,
}

/// Insert a grant. Returns the new row's id + created_at on success. If the
/// idempotency key already exists, returns `Ok(None)` so the caller can fetch
/// the prior row.
///
/// Takes any `PgExecutor` so the grant handler can run the insert inside the
/// advisory-locked transaction that also runs the cap check (CPL-379 L5).
pub async fn insert(
    executor: impl PgExecutor<'_>,
    g: &NewGrant<'_>,
) -> Result<Option<(i64, OffsetDateTime)>> {
    let row: Option<(i64, OffsetDateTime)> = sqlx::query_as(
        "INSERT INTO grants (operator_id, stripe_customer_id, wallet_address, email, \
                             cents, note, stripe_balance_transaction_id, idempotency_key) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (idempotency_key) DO NOTHING \
         RETURNING id, created_at",
    )
    .bind(g.operator_id)
    .bind(g.stripe_customer_id)
    .bind(g.wallet_address)
    .bind(g.email)
    .bind(g.cents)
    .bind(g.note)
    .bind(g.stripe_balance_transaction_id)
    .bind(g.idempotency_key)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Look up a grant by idempotency key. Used to detect retries when the prior
/// attempt got far enough to write the row. Takes any `PgExecutor` so the
/// lookup can run inside the grant handler's advisory-locked transaction
/// (CPL-379 L5).
pub async fn find_by_idempotency_key(
    executor: impl PgExecutor<'_>,
    idempotency_key: &str,
) -> Result<Option<GrantRow>> {
    let row = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<String>,
            i64,
            String,
            String,
            OffsetDateTime,
        ),
    >(
        "SELECT id, stripe_customer_id, wallet_address, email, cents, note, \
                stripe_balance_transaction_id, created_at \
         FROM grants WHERE idempotency_key = $1",
    )
    .bind(idempotency_key)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(map_grant_row))
}

/// Recent grants by a given operator, newest first.
pub async fn list_recent_by_operator(
    pool: &PgPool,
    operator_id: i64,
    limit: i64,
) -> Result<Vec<GrantRow>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<String>,
            i64,
            String,
            String,
            OffsetDateTime,
        ),
    >(
        "SELECT id, stripe_customer_id, wallet_address, email, cents, note, \
                stripe_balance_transaction_id, created_at \
         FROM grants \
         WHERE operator_id = $1 \
         ORDER BY created_at DESC \
         LIMIT $2",
    )
    .bind(operator_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_grant_row).collect())
}

fn map_grant_row(
    r: (
        i64,
        String,
        String,
        Option<String>,
        i64,
        String,
        String,
        OffsetDateTime,
    ),
) -> GrantRow {
    GrantRow {
        id: r.0,
        stripe_customer_id: r.1,
        wallet_address: r.2,
        email: r.3,
        cents: r.4,
        note: r.5,
        stripe_balance_transaction_id: r.6,
        created_at: r.7,
    }
}
