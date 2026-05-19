//! Pure formatting helpers used across billing services.

/// Format cents as a display string, e.g. 500 → "$5.00".
///
/// NOTE: known sign-loss bug for values in -99..=-1 (integer division truncates
/// toward zero, so -1/100 = 0, losing the minus sign). Also, the format "$-5.00"
/// is non-standard (convention is "-$5.00"). Preserved as-is for behavior
/// compatibility with the existing `lit-api-server` callers.
pub fn cents_to_display(cents: i64) -> String {
    format!("${}.{:02}", cents / 100, cents.abs() % 100)
}

/// Convert a Unix timestamp (seconds, UTC) to a `YYYY-MM-DD` date string.
///
/// Pure function — no external deps (avoids pulling `chrono` into billing
/// services).
pub fn unix_to_utc_date(ts: i64) -> String {
    // Days since 1970-01-01 (Thursday), floor.
    let days = ts.div_euclid(86_400);
    // Convert to (year, month, day) using Howard Hinnant's civil_from_days algorithm.
    // https://howardhinnant.github.io/date_algorithms.html#civil_from_days
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_to_display_whole_dollars() {
        assert_eq!(cents_to_display(500), "$5.00");
        assert_eq!(cents_to_display(100), "$1.00");
        assert_eq!(cents_to_display(0), "$0.00");
    }

    #[test]
    fn cents_to_display_with_cents() {
        assert_eq!(cents_to_display(199), "$1.99");
        assert_eq!(cents_to_display(1), "$0.01");
        assert_eq!(cents_to_display(50), "$0.50");
    }

    #[test]
    fn cents_to_display_negative() {
        // Documenting current behavior — see fn-level NOTE.
        assert_eq!(cents_to_display(-500), "$-5.00");
        assert_eq!(cents_to_display(-1), "$0.01");
    }

    #[test]
    fn unix_to_utc_date_epoch() {
        assert_eq!(unix_to_utc_date(0), "1970-01-01");
    }

    #[test]
    fn unix_to_utc_date_known_values() {
        // 2026-04-21 00:00:00 UTC = 1_776_729_600
        assert_eq!(unix_to_utc_date(1_776_729_600), "2026-04-21");
        // 2026-04-21 23:59:59 UTC
        assert_eq!(unix_to_utc_date(1_776_729_600 + 86_399), "2026-04-21");
        // 2026-04-22 00:00:00 UTC
        assert_eq!(unix_to_utc_date(1_776_729_600 + 86_400), "2026-04-22");
    }

    #[test]
    fn unix_to_utc_date_leap_year() {
        // 2024-02-29 is a leap day.  1709164800 = 2024-02-29 00:00:00 UTC
        assert_eq!(unix_to_utc_date(1_709_164_800), "2024-02-29");
        assert_eq!(unix_to_utc_date(1_709_251_199), "2024-02-29");
        assert_eq!(unix_to_utc_date(1_709_251_200), "2024-03-01");
    }
}
