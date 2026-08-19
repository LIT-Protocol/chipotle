//! Sub-cent metering (plans/tee-chat-app.md section 8.2) — P1 runs this in
//! shadow mode: the accumulator is real, no payment path exists yet.
//!
//! - Per-user accumulator in micro-USD, encrypted under the user's KEK
//!   (usage patterns are content-adjacent).
//! - Aggregate, non-attributable day totals feed the global spend breaker.
//! - Settlement happens in-request at end of stream (the enclave holds the
//!   user's KEK during their session). Never kills a stream for a billing
//!   failure: settlement errors are logged and absorbed.

use crate::config::Config;
use crate::store::{meters, settings};
use crate::{envelope, identity::UserKind};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeterState {
    /// Lifetime accumulated cost (cost x (1 + markup)), micro-USD. The exact
    /// remainder is kept here; a future Stripe flush takes
    /// floor(micro_usd / 10_000) whole cents and carries the rest forward.
    pub micro_usd_total: i64,
    /// Current budget day (UTC, YYYY-MM-DD).
    pub day: String,
    /// Tokens consumed today (anonymous budget input).
    pub day_tokens: i64,
    pub day_micro_usd: i64,
}

pub fn today_utc() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        now.month() as u8,
        now.day()
    )
}

/// Apply the markup: cost x (1 + markup_bps/10_000), floored.
pub fn with_markup(cost_micro_usd: i64, markup_bps: i64) -> i64 {
    cost_micro_usd + (cost_micro_usd * markup_bps) / 10_000
}

pub struct BudgetDecision {
    pub allowed: bool,
    /// Stable machine-readable reason when denied.
    pub reason: &'static str,
}

/// Pre-flight gate (reserve-before shape): breaker state, then the caller's
/// daily budget.
pub async fn check_budget(
    pool: &PgPool,
    cfg: &Config,
    kek: &[u8; 32],
    user_ref_hash: &str,
    kind: UserKind,
) -> Result<BudgetDecision> {
    let defaults = settings::Caps {
        daily_spend_cap_micro_usd: cfg.daily_spend_cap_micro_usd,
        anon_daily_token_budget: cfg.anon_daily_token_budget,
    };
    let caps = settings::caps(pool, &defaults).await?;
    let mode = settings::breaker_mode(pool).await?;
    let today = today_utc();
    let day_total = meters::day_spend(pool, &today).await?;

    let effective = match mode {
        settings::BreakerMode::ForcedOff => "off",
        settings::BreakerMode::ForcedAccountsOnly => "accounts_only",
        settings::BreakerMode::ForcedOn => "on",
        settings::BreakerMode::Auto => {
            if day_total >= caps.daily_spend_cap_micro_usd {
                "off"
            } else if day_total >= (caps.daily_spend_cap_micro_usd * 8) / 10 {
                // Degrade to account holders only before hard-off (section 4.2).
                "accounts_only"
            } else {
                "on"
            }
        }
    };
    match (effective, kind) {
        ("off", _) => {
            return Ok(BudgetDecision {
                allowed: false,
                reason: "spend_breaker_open",
            })
        }
        ("accounts_only", UserKind::Anon) => {
            return Ok(BudgetDecision {
                allowed: false,
                reason: "spend_breaker_accounts_only",
            })
        }
        _ => {}
    }

    if kind == UserKind::Anon {
        let state = load(pool, kek, user_ref_hash).await?.0;
        let day_tokens = if state.day == today {
            state.day_tokens
        } else {
            0
        };
        if day_tokens >= caps.anon_daily_token_budget {
            return Ok(BudgetDecision {
                allowed: false,
                // Exhausting the budget is an upgrade prompt, not a paywall.
                reason: "anon_daily_budget_exhausted",
            });
        }
    }
    Ok(BudgetDecision {
        allowed: true,
        reason: "ok",
    })
}

pub async fn load(
    pool: &PgPool,
    kek: &[u8; 32],
    user_ref_hash: &str,
) -> Result<(MeterState, Option<i64>)> {
    match meters::get(pool, user_ref_hash).await? {
        Some((enc, version)) => {
            let json = envelope::decrypt_meter(kek, user_ref_hash, &enc)?;
            let state = serde_json::from_str(&json).unwrap_or_default();
            Ok((state, Some(version)))
        }
        None => Ok((MeterState::default(), None)),
    }
}

/// Settle a completed generation into the user's encrypted accumulator and
/// the aggregate day counter. CAS with a small bounded retry.
pub async fn settle(
    pool: &PgPool,
    cfg: &Config,
    kek: &[u8; 32],
    user_ref_hash: &str,
    cost_micro_usd: i64,
    tokens: i64,
) -> Result<()> {
    let charged = with_markup(cost_micro_usd, cfg.markup_bps);
    let today = today_utc();
    for _ in 0..3 {
        let (mut state, version) = load(pool, kek, user_ref_hash).await?;
        if state.day != today {
            state.day = today.clone();
            state.day_tokens = 0;
            state.day_micro_usd = 0;
        }
        state.micro_usd_total += charged;
        state.day_tokens += tokens;
        state.day_micro_usd += charged;
        let enc = envelope::encrypt_meter(kek, user_ref_hash, &serde_json::to_string(&state)?)?;
        if meters::put(pool, user_ref_hash, &enc, version).await? {
            meters::add_day_spend(pool, &today, charged).await?;
            return Ok(());
        }
    }
    // Absorb after retries — never fail the request path over metering.
    tracing::warn!(
        user_ref_hash,
        "meter settlement lost CAS race 3x; absorbing"
    );
    meters::add_day_spend(pool, &today, charged).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_floors_and_never_rounds_up() {
        assert_eq!(with_markup(100, 500), 105);
        assert_eq!(with_markup(1, 500), 1); // 1.05 floors to 1
        assert_eq!(with_markup(0, 500), 0);
        assert_eq!(with_markup(19, 500), 19); // 19.95 floors to 19
        assert_eq!(with_markup(20, 500), 21);
    }

    #[test]
    fn day_format() {
        let d = today_utc();
        assert_eq!(d.len(), 10);
        assert_eq!(&d[4..5], "-");
    }
}
