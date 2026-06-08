//! Postgres queries against `auto_topup_config` and `auto_topup_credits`.
//!
//! The CHECK constraint on `auto_topup_config` (migration §4) is the source
//! of truth for "enabled requires complete config" — we deliberately let
//! the DB reject bad combinations rather than duplicating the logic in
//! application code. The handler maps the `check_violation` SQLSTATE
//! (`23514`) to a 400 with a clear message.

use anyhow::Result;
use sqlx::PgPool;
use time::OffsetDateTime;

use super::types::{AutoTopupConfigRow, AutoTopupConfigUpsert};

type Row = (
    String,                 // customer_id
    String,                 // wallet_address
    bool,                   // enabled
    Option<i64>,            // threshold_cents
    Option<i64>,            // topup_amount_cents
    Option<i64>,            // monthly_cap_cents
    Option<String>,         // payment_method_id
    Option<String>,         // consent_version
    Option<OffsetDateTime>, // consent_signed_at
    Option<String>,         // disabled_reason
    Option<String>,         // pending_action_pi_id
    Option<OffsetDateTime>, // pending_action_at
    Option<String>,         // recovery_token
    Option<OffsetDateTime>, // recovery_token_expires_at
    OffsetDateTime,         // updated_at
);

fn into_row(r: Row) -> AutoTopupConfigRow {
    AutoTopupConfigRow {
        customer_id: r.0,
        wallet_address: r.1,
        enabled: r.2,
        threshold_cents: r.3,
        topup_amount_cents: r.4,
        monthly_cap_cents: r.5,
        payment_method_id: r.6,
        consent_version: r.7,
        consent_signed_at: r.8,
        disabled_reason: r.9,
        pending_action_pi_id: r.10,
        pending_action_at: r.11,
        recovery_token: r.12,
        recovery_token_expires_at: r.13,
        updated_at: r.14,
    }
}

const SELECT_COLUMNS: &str = "customer_id, wallet_address, enabled, \
    threshold_cents, topup_amount_cents, monthly_cap_cents, payment_method_id, \
    consent_version, consent_signed_at, disabled_reason, \
    pending_action_pi_id, pending_action_at, recovery_token, \
    recovery_token_expires_at, updated_at";

pub async fn get_by_customer_id(
    pool: &PgPool,
    customer_id: &str,
) -> Result<Option<AutoTopupConfigRow>> {
    let row: Option<Row> = sqlx::query_as(&format!(
        "SELECT {SELECT_COLUMNS} FROM auto_topup_config WHERE customer_id = $1"
    ))
    .bind(customer_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(into_row))
}

/// UPSERT the per-user config row.
///
/// Behavior on transition to `enabled = false`:
///   * `disabled_reason` set to `'manual'`
///   * `pending_action_pi_id`, `pending_action_at`, `recovery_token`,
///     `recovery_token_expires_at` are all NULLed
///
/// The reset closes the gap codex flagged ("stale SCA/recovery state can
/// trigger later charges"): once a user opts out, any pending off-session
/// authentication handoff is moot, so the row should not carry the bait
/// that would re-engage the flow when the user re-enables.
///
/// `wallet_address` is required so a brand-new row can be inserted; for
/// existing rows the UPDATE branch keeps the existing wallet_address (the
/// caller's resolved wallet always matches it anyway thanks to the UNIQUE
/// constraint).
pub async fn upsert(
    pool: &PgPool,
    customer_id: &str,
    wallet_address: &str,
    body: &AutoTopupConfigUpsert,
) -> Result<AutoTopupConfigRow> {
    // `consent_signed_at` is recorded now (server time) — never trusted
    // from the client.
    let now = OffsetDateTime::now_utc();
    let consent_signed_at = if body.enabled { Some(now) } else { None };

    let (disabled_reason, pending_pi, pending_at, token, token_exp) = if body.enabled {
        // Re-enabling: don't carry forward an old disabled_reason; pending
        // state, if any, stays — a webhook handler may still need it.
        (
            None::<String>,
            None::<String>,
            None::<OffsetDateTime>,
            None::<String>,
            None::<OffsetDateTime>,
        )
    } else {
        // Disabling: 'manual' is the user-initiated reason. Pending state
        // is cleared (see doc comment).
        (Some("manual".to_string()), None, None, None, None)
    };

    let row: Row = sqlx::query_as(&format!(
        "INSERT INTO auto_topup_config (\
            customer_id, wallet_address, enabled, threshold_cents, \
            topup_amount_cents, monthly_cap_cents, payment_method_id, \
            consent_version, consent_signed_at, disabled_reason, \
            pending_action_pi_id, pending_action_at, recovery_token, \
            recovery_token_expires_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
         ON CONFLICT (customer_id) DO UPDATE SET \
            enabled = EXCLUDED.enabled, \
            threshold_cents = EXCLUDED.threshold_cents, \
            topup_amount_cents = EXCLUDED.topup_amount_cents, \
            monthly_cap_cents = EXCLUDED.monthly_cap_cents, \
            payment_method_id = EXCLUDED.payment_method_id, \
            consent_version = EXCLUDED.consent_version, \
            consent_signed_at = EXCLUDED.consent_signed_at, \
            disabled_reason = EXCLUDED.disabled_reason, \
            pending_action_pi_id = EXCLUDED.pending_action_pi_id, \
            pending_action_at = EXCLUDED.pending_action_at, \
            recovery_token = EXCLUDED.recovery_token, \
            recovery_token_expires_at = EXCLUDED.recovery_token_expires_at, \
            updated_at = EXCLUDED.updated_at \
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(customer_id)
    .bind(wallet_address)
    .bind(body.enabled)
    .bind(body.threshold_cents)
    .bind(body.topup_amount_cents)
    .bind(body.monthly_cap_cents)
    .bind(body.payment_method_id.as_deref())
    .bind(body.consent_version.as_deref())
    .bind(consent_signed_at)
    .bind(disabled_reason)
    .bind(pending_pi)
    .bind(pending_at)
    .bind(token)
    .bind(token_exp)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(into_row(row))
}

/// Returns true if the error is a Postgres CHECK constraint violation
/// (SQLSTATE 23514) on the `enabled_requires_config` constraint. Lets
/// handlers map "you said enabled=true but didn't set all required fields"
/// to a 400 cleanly.
pub fn is_check_constraint_violation(e: &anyhow::Error) -> bool {
    e.downcast_ref::<sqlx::Error>()
        .and_then(|e| match e {
            sqlx::Error::Database(db) => Some(db),
            _ => None,
        })
        .and_then(|db| db.code())
        .map(|code| code == "23514")
        .unwrap_or(false)
}
