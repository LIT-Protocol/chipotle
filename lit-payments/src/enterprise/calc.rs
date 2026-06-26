//! Enterprise billing arithmetic (pure, integer-exact).
//!
//! The payer Stripe customer is debited at the standard $0.01/unit rate (1 cent
//! == 1 unit). Because the billing job keeps the payer's credit topped to
//! `target_credit_cents` with exactly ONE regrant per cycle, the amount consumed
//! during the cycle is recoverable from the current balance alone.
//!
//! Stripe balances are signed: negative == credit available, positive == owed.

/// Units consumed since the buffer was last restored to target.
///
/// `consumed = target + balance` (balance is negative while credit remains, so
/// this subtracts the remaining credit from the target). Clamped at 0: if the
/// account somehow holds *more* than target credit, consumption reads as 0
/// rather than negative.
pub fn consumed_units(target_credit_cents: i64, balance_cents: i64) -> i64 {
    target_credit_cents.saturating_add(balance_cents).max(0)
}

/// Units beyond the included allotment.
pub fn overage_units(consumed: i64, included: i64) -> i64 {
    consumed.saturating_sub(included).max(0)
}

/// Overage charge in cents, rounded to the nearest cent. The rate is expressed
/// in hundredths-of-a-cent per unit ($0.0025 == 25) to keep this exact.
pub fn overage_cents(overage_units: i64, rate_hundredths_cent_per_unit: i64) -> i64 {
    (overage_units.saturating_mul(rate_hundredths_cent_per_unit) + 50) / 100
}

/// Signed balance-transaction amount that restores the buffer to target.
/// Negative == credit. Returns 0 when the account already holds ≥ target credit
/// (we never claw credit back).
pub fn regrant_amount_cents(target_credit_cents: i64, balance_cents: i64) -> i64 {
    let amount = -(target_credit_cents.saturating_add(balance_cents));
    amount.min(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // $500k target, ~$470k credit remaining → consumed exactly the 3M allotment.
    #[test]
    fn consumed_at_allotment() {
        assert_eq!(consumed_units(50_000_000, -47_000_000), 3_000_000);
    }

    #[test]
    fn consumed_clamps_when_over_credited() {
        // More credit than target → 0, never negative.
        assert_eq!(consumed_units(50_000_000, -60_000_000), 0);
    }

    #[test]
    fn no_overage_at_or_below_allotment() {
        assert_eq!(overage_units(3_000_000, 3_000_000), 0);
        assert_eq!(overage_units(2_500_000, 3_000_000), 0);
    }

    // 1,000,000 units over @ $0.0025 = $2,500 = 250,000 cents.
    #[test]
    fn overage_million_units() {
        let over = overage_units(4_000_000, 3_000_000);
        assert_eq!(over, 1_000_000);
        assert_eq!(overage_cents(over, 25), 250_000);
    }

    // Rounding: 3 units @ 25 hundredths-cent = 75 hundredths-cent = 0.75¢ → 1¢.
    #[test]
    fn overage_rounds_to_nearest_cent() {
        assert_eq!(overage_cents(3, 25), 1);
        assert_eq!(overage_cents(1, 25), 0); // 0.25¢ → 0¢
        assert_eq!(overage_cents(2, 25), 1); // 0.50¢ → 1¢ (round half up)
    }

    // Regrant restores -$470k credit back to -$500k by crediting $30k.
    #[test]
    fn regrant_restores_to_target() {
        assert_eq!(regrant_amount_cents(50_000_000, -47_000_000), -3_000_000);
    }

    #[test]
    fn regrant_noop_when_above_target() {
        assert_eq!(regrant_amount_cents(50_000_000, -60_000_000), 0);
    }

    // Consumed and the (negated) regrant magnitude agree.
    #[test]
    fn regrant_magnitude_equals_consumed() {
        let target = 50_000_000;
        let balance = -47_000_000;
        assert_eq!(-regrant_amount_cents(target, balance), consumed_units(target, balance));
    }
}
