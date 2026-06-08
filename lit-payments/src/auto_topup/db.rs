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

    // Codex P1 (Phase 4) fix: pending SCA recovery state belongs to the
    // server, not the client. A normal enabled save during
    // `requires_action` must NOT wipe `pending_action_pi_id` /
    // `recovery_token` / etc. The SQL `ON CONFLICT DO UPDATE` below uses
    // a CASE expression on each pending field to preserve the existing
    // value when the upsert keeps the row enabled, and only clears them
    // when the user explicitly disables. We also keep `disabled_reason`
    // intact while it is `requires_action` so the dashboard's SCA banner
    // logic stays correct; other disabled_reasons ('manual', 'failures')
    // are cleared on re-enable as the user expects.
    let new_disabled_reason: Option<String> = if body.enabled {
        None
    } else {
        Some("manual".to_string())
    };

    let row: Row = sqlx::query_as(&format!(
        "INSERT INTO auto_topup_config (\
            customer_id, wallet_address, enabled, threshold_cents, \
            topup_amount_cents, monthly_cap_cents, payment_method_id, \
            consent_version, consent_signed_at, disabled_reason, \
            pending_action_pi_id, pending_action_at, recovery_token, \
            recovery_token_expires_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NULL, NULL, NULL, NULL, $11) \
         ON CONFLICT (customer_id) DO UPDATE SET \
            enabled = EXCLUDED.enabled, \
            threshold_cents = EXCLUDED.threshold_cents, \
            topup_amount_cents = EXCLUDED.topup_amount_cents, \
            monthly_cap_cents = EXCLUDED.monthly_cap_cents, \
            payment_method_id = EXCLUDED.payment_method_id, \
            consent_version = EXCLUDED.consent_version, \
            consent_signed_at = EXCLUDED.consent_signed_at, \
            disabled_reason = CASE \
                WHEN NOT EXCLUDED.enabled THEN EXCLUDED.disabled_reason \
                WHEN auto_topup_config.disabled_reason = 'requires_action' THEN 'requires_action' \
                ELSE NULL \
            END, \
            pending_action_pi_id = CASE \
                WHEN EXCLUDED.enabled THEN auto_topup_config.pending_action_pi_id \
                ELSE NULL \
            END, \
            pending_action_at = CASE \
                WHEN EXCLUDED.enabled THEN auto_topup_config.pending_action_at \
                ELSE NULL \
            END, \
            recovery_token = CASE \
                WHEN EXCLUDED.enabled THEN auto_topup_config.recovery_token \
                ELSE NULL \
            END, \
            recovery_token_expires_at = CASE \
                WHEN EXCLUDED.enabled THEN auto_topup_config.recovery_token_expires_at \
                ELSE NULL \
            END, \
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
    .bind(new_disabled_reason)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(into_row(row))
}

/// Mark the config as auto-disabled after the consecutive-failure
/// threshold is crossed. Idempotent — the WHERE clause means a second call
/// is a no-op even if the row was already disabled.
pub async fn disable_after_failures(pool: &PgPool, customer_id: &str) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    sqlx::query(
        "UPDATE auto_topup_config \
            SET enabled = false, \
                disabled_reason = 'failures', \
                pending_action_pi_id = NULL, \
                pending_action_at = NULL, \
                recovery_token = NULL, \
                recovery_token_expires_at = NULL, \
                updated_at = $1 \
            WHERE customer_id = $2",
    )
    .bind(now)
    .bind(customer_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record an off-session `authentication_required` handoff: store the
/// pending PI id + a fresh recovery token (24h TTL). The auto top-up rule
/// stays in the DB but is functionally paused — `disabled_reason` is
/// `'requires_action'`, the dashboard shows the "action required" banner,
/// and any new `customer.updated` will short-circuit on the
/// disabled_reason check.
pub async fn set_pending_action(
    pool: &PgPool,
    customer_id: &str,
    pi_id: &str,
    recovery_token: &str,
) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    let expires = now + time::Duration::hours(24);
    sqlx::query(
        "UPDATE auto_topup_config \
            SET pending_action_pi_id = $1, \
                pending_action_at = $2, \
                recovery_token = $3, \
                recovery_token_expires_at = $4, \
                disabled_reason = 'requires_action', \
                updated_at = $2 \
            WHERE customer_id = $5",
    )
    .bind(pi_id)
    .bind(now)
    .bind(recovery_token)
    .bind(expires)
    .bind(customer_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically clear the pending-action state after the SCA recovery flow
/// credits a previously-pending PI. The `pending_action_pi_id` WHERE
/// predicate makes the write a no-op if some other concurrent path
/// already cleared it.
pub async fn clear_pending_action(pool: &PgPool, customer_id: &str, pi_id: &str) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    sqlx::query(
        "UPDATE auto_topup_config \
            SET pending_action_pi_id = NULL, \
                pending_action_at = NULL, \
                recovery_token = NULL, \
                recovery_token_expires_at = NULL, \
                disabled_reason = NULL, \
                updated_at = $1 \
            WHERE customer_id = $2 AND pending_action_pi_id = $3",
    )
    .bind(now)
    .bind(customer_id)
    .bind(pi_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// INSERT a row into `auto_topup_credits` (PK on `payment_intent_id`) to
/// claim the credit slot. Returns `Ok(true)` if the row was inserted,
/// `Ok(false)` if a row already existed (dedup hit — concurrent webhook
/// + reconciler, or a webhook replay). Atomic — the unique constraint is
/// the durable correctness primitive (see plan §11).
pub async fn try_insert_credit(
    pool: &PgPool,
    payment_intent_id: &str,
    customer_id: &str,
    amount_cents: i64,
) -> Result<bool> {
    let inserted: Option<(String,)> = sqlx::query_as(
        "INSERT INTO auto_topup_credits (payment_intent_id, customer_id, amount_cents) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (payment_intent_id) DO NOTHING \
         RETURNING payment_intent_id",
    )
    .bind(payment_intent_id)
    .bind(customer_id)
    .bind(amount_cents)
    .fetch_optional(pool)
    .await?;
    Ok(inserted.is_some())
}

/// Mark a previously-inserted credit row as fully credited (the
/// `balance_transactions` write succeeded). The reconciler distinguishes
/// "credit row exists, balance_tx_id NULL" (partial — retry) from
/// "credit row exists with balance_tx_id" (done — skip).
pub async fn mark_credit_completed(
    pool: &PgPool,
    payment_intent_id: &str,
    stripe_balance_transaction_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE auto_topup_credits \
            SET stripe_balance_transaction_id = $1 \
            WHERE payment_intent_id = $2",
    )
    .bind(stripe_balance_transaction_id)
    .bind(payment_intent_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// List all customer ids with `enabled = true`. Used by the reconciler
/// to scope its PI scan to actually-active subscriptions.
pub async fn list_enabled_customers(pool: &PgPool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT customer_id FROM auto_topup_config WHERE enabled = true")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Codex P1 (Phase 6): reconciler must also visit customers who have a
/// partial credit row (PI created, balance_tx never landed) even if the
/// row has since been disabled — failures-threshold auto-disable flips
/// `enabled=false`, but the orphaned credit row still needs the
/// balance_transactions retry. Pre-fix the reconciler scoped exclusively
/// to enabled customers and these stayed pending forever.
pub async fn list_customers_with_partial_credits(pool: &PgPool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT customer_id FROM auto_topup_credits \
            WHERE stripe_balance_transaction_id IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Per-PI credit row (subset). `None` means no row exists yet.
#[derive(Debug, Clone)]
pub struct CreditRow {
    pub payment_intent_id: String,
    pub customer_id: String,
    pub amount_cents: i64,
    pub stripe_balance_transaction_id: Option<String>,
    /// Used by the reconciler to age-gate partial retries (Codex P1
    /// Phase 6). The webhook handler typically commits the balance_tx
    /// within a few hundred ms of inserting the credit row, so a fresh
    /// partial row is overwhelmingly likely to be the live webhook
    /// flow's in-flight write, not a real orphan.
    pub credited_at: OffsetDateTime,
}

/// Fetch the credit row for a PI. Used by the reconciler to triage
/// orphans (no row, or row with null balance_tx_id).
pub async fn find_credit_row(pool: &PgPool, payment_intent_id: &str) -> Result<Option<CreditRow>> {
    let row: Option<(String, String, i64, Option<String>, OffsetDateTime)> = sqlx::query_as(
        "SELECT payment_intent_id, customer_id, amount_cents, \
                stripe_balance_transaction_id, credited_at \
         FROM auto_topup_credits WHERE payment_intent_id = $1",
    )
    .bind(payment_intent_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| CreditRow {
        payment_intent_id: r.0,
        customer_id: r.1,
        amount_cents: r.2,
        stripe_balance_transaction_id: r.3,
        credited_at: r.4,
    }))
}

/// SCA recovery: resolve a `recovery_token` to the
/// `(customer_id, pending_action_pi_id)` it grants access to, WITHOUT
/// invalidating it. Used by the GET resume endpoint to fetch the PI's
/// client_secret from Stripe; the caller invalidates only after the
/// Stripe call succeeds via [`clear_recovery_token_for_pi`].
///
/// Splitting lookup from invalidation (codex P2 #3) preserves the
/// single-use semantic for the happy path while letting a transient
/// Stripe failure leave the token usable for retry. A successful Stripe
/// call always burns the token; a 503 from Stripe never does.
pub async fn lookup_recovery_token(pool: &PgPool, token: &str) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT customer_id, pending_action_pi_id FROM auto_topup_config \
            WHERE recovery_token = $1 \
              AND recovery_token_expires_at > now() \
              AND pending_action_pi_id IS NOT NULL",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Invalidate the recovery token for the given (customer, pi) pair. The
/// WHERE clause matches on `pending_action_pi_id` so a race that landed
/// on a different PI cannot accidentally clear the wrong row's token.
pub async fn clear_recovery_token_for_pi(
    pool: &PgPool,
    customer_id: &str,
    pi_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE auto_topup_config \
            SET recovery_token = NULL, recovery_token_expires_at = NULL \
            WHERE customer_id = $1 AND pending_action_pi_id = $2",
    )
    .bind(customer_id)
    .bind(pi_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Legacy atomic consume — kept for callers that already had the
/// preserve-on-failure semantics handled at their level. Prefer the
/// lookup + clear split above.
#[allow(dead_code)]
pub async fn consume_recovery_token(
    pool: &PgPool,
    token: &str,
) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "UPDATE auto_topup_config \
            SET recovery_token = NULL, recovery_token_expires_at = NULL \
            WHERE recovery_token = $1 \
              AND recovery_token_expires_at > now() \
              AND pending_action_pi_id IS NOT NULL \
            RETURNING customer_id, pending_action_pi_id",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
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
