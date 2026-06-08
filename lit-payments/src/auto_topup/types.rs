//! Strongly-typed view of an `auto_topup_config` row.
//!
//! `cents` everywhere — never dollars. The dashboard converts at the edge.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Full snapshot of an `auto_topup_config` row.
///
/// Optional fields reflect the schema: when `enabled = false`, all
/// configuration fields may be NULL. The CHECK constraint on the table
/// guarantees that `enabled = true` rows have every required field set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTopupConfigRow {
    pub customer_id: String,
    pub wallet_address: String,
    pub enabled: bool,
    pub threshold_cents: Option<i64>,
    pub topup_amount_cents: Option<i64>,
    pub monthly_cap_cents: Option<i64>,
    pub payment_method_id: Option<String>,
    pub consent_version: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub consent_signed_at: Option<OffsetDateTime>,
    pub disabled_reason: Option<String>,
    pub pending_action_pi_id: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub pending_action_at: Option<OffsetDateTime>,
    pub recovery_token: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub recovery_token_expires_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Caller-supplied fields for the `PUT /billing/auto_topup_config` upsert.
///
/// Only the user-controllable fields appear here. Server-derived fields
/// (`pending_action_pi_id`, `recovery_token`, `disabled_reason`,
/// `updated_at`) are managed by the webhook handler and the disable
/// transition — the dashboard never writes them directly.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoTopupConfigUpsert {
    pub enabled: bool,
    pub threshold_cents: Option<i64>,
    pub topup_amount_cents: Option<i64>,
    pub monthly_cap_cents: Option<i64>,
    pub payment_method_id: Option<String>,
    pub consent_version: Option<String>,
}
