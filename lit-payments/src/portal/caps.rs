//! Per-grant and per-operator-per-day cap enforcement.

use anyhow::Result;
use sqlx::PgExecutor;

/// Sum of cents granted by this operator in the last 24 hours.
///
/// Accepts any `PgExecutor` so the caller can run this inside the same
/// transaction that holds the per-operator advisory lock (CPL-379 L5): reading
/// the total and inserting the grant under one lock makes the cap check
/// atomic against concurrent grants by the same operator.
pub async fn cents_granted_last_24h(
    executor: impl PgExecutor<'_>,
    operator_id: i64,
) -> Result<i64> {
    // PostgreSQL returns SUM(bigint) as NUMERIC, which sqlx will not decode into
    // i64 directly. Cast after COALESCE so both the empty and non-empty cases
    // return INT8 for Rust.
    let cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(cents), 0)::bigint \
         FROM grants \
         WHERE operator_id = $1 AND created_at > now() - interval '24 hours'",
    )
    .bind(operator_id)
    .fetch_one(executor)
    .await?;
    Ok(cents)
}

/// Outcome of a cap check. `Err(_)` is reserved for server errors; cap
/// violations are returned as `Ok(CapCheck::OverPerGrant | OverDaily)`.
#[derive(Debug, PartialEq, Eq)]
pub enum CapCheck {
    Ok,
    NonPositive,
    OverPerGrant {
        cents: i64,
        max_cents: i64,
    },
    OverDaily {
        cents: i64,
        already_today_cents: i64,
        max_daily_cents: i64,
    },
}

/// Pure cap-decision function. Server fetches `already_today_cents` from the
/// DB, then calls this with the proposed grant amount + configured caps.
pub fn check(
    proposed_cents: i64,
    already_today_cents: i64,
    max_per_grant_cents: i64,
    max_daily_cents: i64,
) -> CapCheck {
    if proposed_cents <= 0 {
        return CapCheck::NonPositive;
    }
    if proposed_cents > max_per_grant_cents {
        return CapCheck::OverPerGrant {
            cents: proposed_cents,
            max_cents: max_per_grant_cents,
        };
    }
    if already_today_cents.saturating_add(proposed_cents) > max_daily_cents {
        return CapCheck::OverDaily {
            cents: proposed_cents,
            already_today_cents,
            max_daily_cents,
        };
    }
    CapCheck::Ok
}

impl CapCheck {
    /// Render a user-facing error message. Returns `None` if the check passed.
    pub fn message(&self) -> Option<String> {
        match self {
            CapCheck::Ok => None,
            CapCheck::NonPositive => Some("Grant amount must be positive.".to_string()),
            CapCheck::OverPerGrant { cents, max_cents } => Some(format!(
                "Grant of {} cents exceeds per-grant cap of {} cents.",
                cents, max_cents
            )),
            CapCheck::OverDaily {
                cents,
                already_today_cents,
                max_daily_cents,
            } => Some(format!(
                "Grant of {} cents would exceed your daily cap of {} cents \
                 ({} cents already granted in the last 24h).",
                cents, max_daily_cents, already_today_cents
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_ok() {
        assert_eq!(check(500, 0, 2000, 10000), CapCheck::Ok);
        assert_eq!(check(500, 9500, 2000, 10000), CapCheck::Ok);
    }

    #[test]
    fn rejects_non_positive() {
        assert_eq!(check(0, 0, 2000, 10000), CapCheck::NonPositive);
        assert_eq!(check(-1, 0, 2000, 10000), CapCheck::NonPositive);
    }

    #[test]
    fn rejects_over_per_grant() {
        match check(2001, 0, 2000, 10000) {
            CapCheck::OverPerGrant { cents, max_cents } => {
                assert_eq!(cents, 2001);
                assert_eq!(max_cents, 2000);
            }
            other => panic!("expected OverPerGrant, got {other:?}"),
        }
    }

    #[test]
    fn rejects_over_daily() {
        match check(500, 9501, 2000, 10000) {
            CapCheck::OverDaily {
                cents,
                already_today_cents,
                max_daily_cents,
            } => {
                assert_eq!(cents, 500);
                assert_eq!(already_today_cents, 9501);
                assert_eq!(max_daily_cents, 10000);
            }
            other => panic!("expected OverDaily, got {other:?}"),
        }
    }

    #[test]
    fn boundary_at_daily_cap_passes() {
        // already 9500 + grant 500 = exactly 10000 → pass.
        assert_eq!(check(500, 9500, 2000, 10000), CapCheck::Ok);
    }

    #[test]
    fn boundary_one_over_daily_cap_fails() {
        // already 9500 + grant 501 = 10001 → fail.
        assert!(matches!(
            check(501, 9500, 2000, 10000),
            CapCheck::OverDaily { .. }
        ));
    }

    #[test]
    fn per_grant_cap_checked_before_daily() {
        // Over per-grant AND over daily — error should name per-grant.
        match check(3000, 8000, 2000, 10000) {
            CapCheck::OverPerGrant { .. } => {}
            other => panic!("expected OverPerGrant first, got {other:?}"),
        }
    }

    #[test]
    fn message_present_when_failed() {
        assert!(check(500, 0, 2000, 10000).message().is_none());
        assert!(check(0, 0, 2000, 10000).message().is_some());
        assert!(check(3000, 0, 2000, 10000).message().is_some());
    }
}
