//! Anchor-to-anchor billing period math (pure).
//!
//! Each enterprise cycle runs anchor→anchor (e.g. the 17th → the 16th). The
//! invoice issued on a given anchor covers the committed fee for the *upcoming*
//! cycle (advance) and the overage for the *just-closed* cycle (arrears). All
//! functions are pure and unit-tested.

use time::{Date, Month};

/// The most recent anchor date on or before `today`. `anchor_day` is clamped to
/// the month length (the DB constrains it to 1..=28, so clamping is belt-and-
/// suspenders).
pub fn current_anchor(today: Date, anchor_day: u8) -> Date {
    let this = anchor_in(today.year(), today.month(), anchor_day);
    if today >= this {
        this
    } else {
        let (y, m) = prev_month(today.year(), today.month());
        anchor_in(y, m, anchor_day)
    }
}

/// The anchor one cycle before `anchor` (start of the arrears window).
pub fn previous_anchor(anchor: Date, anchor_day: u8) -> Date {
    let (y, m) = prev_month(anchor.year(), anchor.month());
    anchor_in(y, m, anchor_day)
}

/// The anchor one cycle after `anchor` (end of the advance window).
pub fn next_anchor(anchor: Date, anchor_day: u8) -> Date {
    let (y, m) = next_month(anchor.year(), anchor.month());
    anchor_in(y, m, anchor_day)
}

/// `'YYYY-MM'` for the anchor's month — the per-period idempotency key.
pub fn period_key(anchor: Date) -> String {
    format!("{:04}-{:02}", anchor.year(), u8::from(anchor.month()))
}

fn anchor_in(year: i32, month: Month, anchor_day: u8) -> Date {
    let dim = month.length(year);
    let day = anchor_day.clamp(1, dim);
    Date::from_calendar_date(year, month, day).expect("clamped anchor day is always valid")
}

fn prev_month(year: i32, month: Month) -> (i32, Month) {
    if month == Month::January {
        (year - 1, Month::December)
    } else {
        (year, month.previous())
    }
}

fn next_month(year: i32, month: Month) -> (i32, Month) {
    if month == Month::December {
        (year + 1, Month::January)
    } else {
        (year, month.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn current_anchor_on_or_after_anchor_is_this_month() {
        assert_eq!(
            current_anchor(date!(2026 - 07 - 17), 17),
            date!(2026 - 07 - 17)
        );
        assert_eq!(
            current_anchor(date!(2026 - 07 - 30), 17),
            date!(2026 - 07 - 17)
        );
    }

    #[test]
    fn current_anchor_before_anchor_is_prev_month() {
        assert_eq!(
            current_anchor(date!(2026 - 07 - 16), 17),
            date!(2026 - 06 - 17)
        );
        assert_eq!(
            current_anchor(date!(2026 - 07 - 01), 17),
            date!(2026 - 06 - 17)
        );
    }

    #[test]
    fn current_anchor_crosses_year_boundary() {
        assert_eq!(
            current_anchor(date!(2026 - 01 - 05), 17),
            date!(2025 - 12 - 17)
        );
    }

    #[test]
    fn prev_and_next_anchor() {
        let a = date!(2026 - 07 - 17);
        assert_eq!(previous_anchor(a, 17), date!(2026 - 06 - 17));
        assert_eq!(next_anchor(a, 17), date!(2026 - 08 - 17));
        // January wraps to previous December.
        assert_eq!(
            previous_anchor(date!(2026 - 01 - 17), 17),
            date!(2025 - 12 - 17)
        );
        // December wraps to next January.
        assert_eq!(
            next_anchor(date!(2026 - 12 - 17), 17),
            date!(2027 - 01 - 17)
        );
    }

    #[test]
    fn anchor_day_clamps_to_short_month() {
        // Anchor 31 in February clamps to the 28th (2026 is not a leap year).
        assert_eq!(
            current_anchor(date!(2026 - 02 - 28), 31),
            date!(2026 - 02 - 28)
        );
    }

    #[test]
    fn period_key_format() {
        assert_eq!(period_key(date!(2026 - 07 - 17)), "2026-07");
        assert_eq!(period_key(date!(2026 - 12 - 17)), "2026-12");
    }
}
