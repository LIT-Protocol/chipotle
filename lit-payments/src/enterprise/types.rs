//! Row types for the enterprise billing tables.

use time::{Date, OffsetDateTime};

/// One committed-use customer (`enterprise_accounts`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EnterpriseAccount {
    pub id: i64,
    pub name: String,
    /// Stripe customer that consumes service and holds the credit buffer.
    pub payer_customer_id: String,
    /// Stripe customer that receives the invoice (a different customer).
    pub invoice_customer_id: String,
    pub committed_fee_cents: i64,
    pub included_units: i64,
    pub overage_rate_hundredths_cent_per_unit: i64,
    pub target_credit_cents: i64,
    pub billing_anchor_day: i32,
    pub notify_email: String,
    /// false: draft + review email for manual send. true: finalize + send
    /// automatically (0-unit cycles are still held as drafts for review).
    pub auto_send: bool,
    pub term_start: Option<Date>,
    pub term_end: Option<Date>,
    /// Set once the one-time buffer-establishment credit has been written.
    pub baseline_granted_at: Option<OffsetDateTime>,
    /// Stamped before the baseline Stripe write; bounds the retry window so a
    /// lost success-record can't double-credit past Stripe's idempotency TTL.
    pub baseline_attempted_at: Option<OffsetDateTime>,
}

/// One generated (or manually recorded) invoice (`enterprise_invoices`). Also
/// the per-period idempotency gate for the billing job.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EnterpriseInvoice {
    pub id: i64,
    pub enterprise_account_id: i64,
    pub period_key: String,
    pub period_start: Date,
    pub period_end: Date,
    pub committed_period: String,
    pub consumed_units: i64,
    pub included_units: i64,
    pub overage_units: i64,
    pub committed_fee_cents: i64,
    pub overage_cents: i64,
    pub total_cents: i64,
    pub stripe_invoice_id: Option<String>,
    pub regrant_balance_txn_id: Option<String>,
    pub status: String,
    pub created_at: OffsetDateTime,
}
