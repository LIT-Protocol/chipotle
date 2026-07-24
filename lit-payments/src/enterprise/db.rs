//! Postgres access for enterprise billing. Runtime `sqlx::query*` (no
//! compile-time macros), matching the rest of this crate.

use anyhow::Result;
use sqlx::PgPool;
use time::{Date, OffsetDateTime};

use super::types::{EnterpriseAccount, EnterpriseInvoice};

const ACCOUNT_COLS: &str = "id, name, payer_customer_id, invoice_customer_id, \
    committed_fee_cents, included_units, overage_rate_hundredths_cent_per_unit, \
    target_credit_cents, billing_anchor_day, notify_email, auto_send, \
    term_start, term_end, baseline_granted_at, baseline_attempted_at";

const INVOICE_COLS: &str = "id, enterprise_account_id, period_key, period_start, period_end, \
    committed_period, consumed_units, included_units, overage_units, \
    committed_fee_cents, overage_cents, total_cents, stripe_invoice_id, \
    regrant_balance_txn_id, status, created_at";

/// All active committed-use accounts.
pub async fn list_active_accounts(pool: &PgPool) -> Result<Vec<EnterpriseAccount>> {
    let rows = sqlx::query_as::<_, EnterpriseAccount>(&format!(
        "SELECT {ACCOUNT_COLS} FROM enterprise_accounts WHERE active = true ORDER BY id"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Stamp the baseline attempt window-start (idempotent: only when unset). Called
/// before the baseline Stripe write so a lost success-record is recoverable
/// within the idempotency window and refused past it.
pub async fn mark_baseline_attempted(pool: &PgPool, account_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE enterprise_accounts \
         SET baseline_attempted_at = now(), updated_at = now() \
         WHERE id = $1 AND baseline_attempted_at IS NULL",
    )
    .bind(account_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record the one-time baseline buffer grant (or `"none"` when already funded).
pub async fn mark_baseline(pool: &PgPool, account_id: i64, balance_txn_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE enterprise_accounts \
         SET baseline_balance_txn_id = $2, baseline_granted_at = now(), updated_at = now() \
         WHERE id = $1 AND baseline_granted_at IS NULL",
    )
    .bind(account_id)
    .bind(balance_txn_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch the invoice row for a period, if any.
pub async fn get_invoice(
    pool: &PgPool,
    account_id: i64,
    period_key: &str,
) -> Result<Option<EnterpriseInvoice>> {
    let row = sqlx::query_as::<_, EnterpriseInvoice>(&format!(
        "SELECT {INVOICE_COLS} FROM enterprise_invoices \
         WHERE enterprise_account_id = $1 AND period_key = $2"
    ))
    .bind(account_id)
    .bind(period_key)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Insert the frozen amount snapshot for a fresh period. `ON CONFLICT DO
/// NOTHING` makes this safe across restarts / a second worker.
#[allow(clippy::too_many_arguments)]
pub async fn insert_pending_invoice(
    pool: &PgPool,
    account_id: i64,
    period_key: &str,
    period_start: Date,
    period_end: Date,
    committed_period: &str,
    consumed_units: i64,
    included_units: i64,
    overage_units: i64,
    committed_fee_cents: i64,
    overage_cents: i64,
    total_cents: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO enterprise_invoices ( \
             enterprise_account_id, period_key, period_start, period_end, committed_period, \
             consumed_units, included_units, overage_units, \
             committed_fee_cents, overage_cents, total_cents, status \
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending') \
         ON CONFLICT (enterprise_account_id, period_key) DO NOTHING",
    )
    .bind(account_id)
    .bind(period_key)
    .bind(period_start)
    .bind(period_end)
    .bind(committed_period)
    .bind(consumed_units)
    .bind(included_units)
    .bind(overage_units)
    .bind(committed_fee_cents)
    .bind(overage_cents)
    .bind(total_cents)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_invoice_stripe_id(pool: &PgPool, id: i64, stripe_invoice_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE enterprise_invoices SET stripe_invoice_id = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(stripe_invoice_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_invoice_regrant_txn(pool: &PgPool, id: i64, txn_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE enterprise_invoices SET regrant_balance_txn_id = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(txn_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_invoice_status(pool: &PgPool, id: i64, status: &str) -> Result<()> {
    sqlx::query("UPDATE enterprise_invoices SET status = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(status)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark the invoice drafted and the review email sent.
pub async fn set_invoice_drafted(pool: &PgPool, id: i64, now: OffsetDateTime) -> Result<()> {
    sqlx::query(
        "UPDATE enterprise_invoices SET status = 'draft', notified_at = $2, updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark the invoice finalized + sent to the customer and the FYI email sent.
pub async fn set_invoice_sent(pool: &PgPool, id: i64, now: OffsetDateTime) -> Result<()> {
    sqlx::query(
        "UPDATE enterprise_invoices SET status = 'sent', notified_at = $2, updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether the payer account has auto-topup enabled — a hard stop, since any
/// non-regrant credit corrupts the `consumed = target + balance` identity.
pub async fn payer_auto_topup_enabled(pool: &PgPool, payer_customer_id: &str) -> Result<bool> {
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT enabled FROM auto_topup_config WHERE customer_id = $1")
            .bind(payer_customer_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(enabled,)| enabled).unwrap_or(false))
}
