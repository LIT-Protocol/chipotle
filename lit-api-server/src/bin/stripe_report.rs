//! `stripe_report` — download the last N days of Stripe billing data and
//! emit a usage-per-day-per-client report.
//!
//! The binary reads `STRIPE_SECRET_KEY` and `STRIPE_PUBLISHABLE_KEY` from the
//! environment (same as the running API server), lists every Stripe customer,
//! fetches each customer's balance transactions in the window, buckets them by
//! UTC date and client, and writes:
//!
//!   • `<out>.csv`  — one row per (date, customer) with charge count + totals
//!   • `<out>.html` — a self-contained pivot table (rows = clients, cols = days)
//!
//! Intended to be run out-of-band from an operator's machine; we deliberately
//! keep the Stripe secret key off the public HTTP surface.
//!
//! Usage:
//!   stripe_report                             # last 30 days → ./stripe-report.{csv,html}
//!   stripe_report --days 7                    # last 7 days
//!   stripe_report --out /tmp/april            # writes /tmp/april.csv and /tmp/april.html
//!   stripe_report --csv-only                  # skip the HTML file

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use lit_api_server::stripe::{
    self, ReportBalanceTx, ReportCustomer, ReportRow, aggregate_report_rows, cents_to_display,
};

const DEFAULT_DAYS: u32 = 30;
const DEFAULT_OUT: &str = "stripe-report";

struct Args {
    days: u32,
    out: String,
    csv_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut days = DEFAULT_DAYS;
    let mut out = DEFAULT_OUT.to_string();
    let mut csv_only = false;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--days" => {
                let v = it
                    .next()
                    .ok_or_else(|| "--days requires a value".to_string())?;
                days = v
                    .parse::<u32>()
                    .map_err(|_| format!("--days: not a positive integer: {v}"))?;
                if days == 0 {
                    return Err("--days must be >= 1".to_string());
                }
            }
            "--out" => {
                out = it
                    .next()
                    .ok_or_else(|| "--out requires a value".to_string())?;
            }
            "--csv-only" => csv_only = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        days,
        out,
        csv_only,
    })
}

fn print_help() {
    println!(
        "stripe_report — download recent Stripe usage as CSV + HTML.

USAGE:
    stripe_report [OPTIONS]

OPTIONS:
    --days <N>       Window size in days (default: {DEFAULT_DAYS}).
    --out <PATH>     Output path prefix (default: {DEFAULT_OUT}).
                     Writes <PATH>.csv and <PATH>.html.
    --csv-only       Skip the HTML file.
    -h, --help       Show this message.

ENVIRONMENT:
    STRIPE_SECRET_KEY       (required) Stripe secret key.
    STRIPE_PUBLISHABLE_KEY  (required) Stripe publishable key.
"
    );
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("run with --help for usage");
            return ExitCode::from(2);
        }
    };

    let Some(stripe_state) = stripe::init() else {
        eprintln!(
            "error: STRIPE_SECRET_KEY and/or STRIPE_PUBLISHABLE_KEY are not set; nothing to report."
        );
        return ExitCode::from(1);
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Snap to UTC midnight so the Stripe query window, the rendered date
    // columns, and the "last N days" label all refer to the same whole days.
    // Without this the first bucket is partial (starts at `now`'s time-of-day)
    // and we end up with N+1 date columns.
    let today_start = now - now.rem_euclid(86_400);
    let since = today_start - (args.days.saturating_sub(1) as i64) * 86_400;
    let since_date = stripe::unix_to_utc_date(since);
    let until_date = stripe::unix_to_utc_date(today_start);

    eprintln!(
        "Fetching Stripe customers and balance transactions for {since_date} .. {until_date} ({} days) …",
        args.days
    );

    let customers = match stripe::list_all_customers(&stripe_state).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: list_all_customers failed: {e}");
            return ExitCode::from(1);
        }
    };
    eprintln!("  {} customers", customers.len());

    let mut transactions: Vec<ReportBalanceTx> = Vec::new();
    for (i, c) in customers.iter().enumerate() {
        match stripe::list_balance_transactions_since(&stripe_state, &c.id, since).await {
            Ok(txs) => {
                if !txs.is_empty() {
                    eprintln!(
                        "  [{}/{}] {} ({}): {} txs",
                        i + 1,
                        customers.len(),
                        c.id,
                        c.wallet_address.as_deref().unwrap_or("-"),
                        txs.len()
                    );
                }
                transactions.extend(txs);
            }
            Err(e) => {
                eprintln!(
                    "  [{}/{}] {}: list_balance_transactions failed: {e}",
                    i + 1,
                    customers.len(),
                    c.id
                );
            }
        }
    }
    eprintln!("  {} transactions total", transactions.len());

    let rows = aggregate_report_rows(&customers, &transactions);

    let csv_path = format!("{}.csv", args.out);
    if let Err(e) = std::fs::write(&csv_path, render_csv(&rows)) {
        eprintln!("error: writing {csv_path}: {e}");
        return ExitCode::from(1);
    }
    eprintln!("Wrote {csv_path} ({} rows)", rows.len());

    if !args.csv_only {
        let html = render_html(&rows, &customers, &since_date, &until_date, args.days);
        let html_path = format!("{}.html", args.out);
        if let Err(e) = std::fs::write(&html_path, html) {
            eprintln!("error: writing {html_path}: {e}");
            return ExitCode::from(1);
        }
        eprintln!("Wrote {html_path}");
    }

    print_stdout_summary(&rows);

    ExitCode::SUCCESS
}

// ─── CSV ─────────────────────────────────────────────────────────────────────

fn render_csv(rows: &[ReportRow]) -> String {
    let mut out = String::from(
        "date,customer_id,wallet_address,email,charges_count,charges_cents,credits_cents\n",
    );
    for r in rows {
        writeln!(
            &mut out,
            "{},{},{},{},{},{},{}",
            csv_escape(&r.date),
            csv_escape(&r.customer_id),
            csv_escape(r.wallet_address.as_deref().unwrap_or("")),
            csv_escape(r.email.as_deref().unwrap_or("")),
            r.charges_count,
            r.charges_cents,
            r.credits_cents,
        )
        .expect("writeln! to String cannot fail");
    }
    out
}

fn csv_escape(s: &str) -> String {
    // Guard against CSV formula injection: a cell starting with =, +, -, @,
    // tab, or CR is executed as a formula by Excel / Sheets / LibreOffice.
    // A malicious customer-controlled field (e.g. email) could otherwise run
    // code or exfiltrate data when the report is opened. Prefix with a
    // single quote to force text interpretation.
    let needs_prefix = matches!(
        s.chars().next(),
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r')
    );
    let prefixed;
    let s: &str = if needs_prefix {
        prefixed = format!("'{s}");
        &prefixed
    } else {
        s
    };
    // Quote on CR too, not just LF: a bare \r embedded in a field can otherwise
    // confuse CSV consumers that treat it as a record separator.
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

// ─── Summary to stdout ───────────────────────────────────────────────────────

fn print_stdout_summary(rows: &[ReportRow]) {
    let mut total_cents: i64 = 0;
    let mut total_charges: u64 = 0;
    let mut per_customer: BTreeMap<String, (i64, u64)> = BTreeMap::new();
    for r in rows {
        total_cents += r.charges_cents;
        total_charges += r.charges_count;
        let e = per_customer.entry(r.customer_id.clone()).or_insert((0, 0));
        e.0 += r.charges_cents;
        e.1 += r.charges_count;
    }
    println!();
    println!(
        "Total usage: {} across {} charge(s), {} client(s)",
        cents_to_display(total_cents),
        total_charges,
        per_customer.len()
    );
}

// ─── HTML ────────────────────────────────────────────────────────────────────

/// Render a self-contained HTML file with a pivot table
/// (rows = clients, columns = days in the window).
fn render_html(
    rows: &[ReportRow],
    customers: &[ReportCustomer],
    since_date: &str,
    until_date: &str,
    days: u32,
) -> String {
    // Columns: every date in the window [since, until] inclusive.
    let all_dates: Vec<String> = enumerate_dates(since_date, until_date);
    // Also include any extra dates that appear in rows (defensive, in case of
    // clock skew between Stripe and the local machine).
    let mut date_set: BTreeSet<String> = all_dates.iter().cloned().collect();
    for r in rows {
        date_set.insert(r.date.clone());
    }
    let dates: Vec<String> = date_set.into_iter().collect();

    // Rows: one per (customer_id).
    #[derive(Default)]
    struct ClientAgg {
        wallet: Option<String>,
        email: Option<String>,
        per_day_cents: BTreeMap<String, i64>,
        per_day_count: BTreeMap<String, u64>,
        total_cents: i64,
        total_charges: u64,
    }
    let customer_by_id: std::collections::HashMap<&str, &ReportCustomer> =
        customers.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut clients: BTreeMap<String, ClientAgg> = BTreeMap::new();
    for r in rows {
        let agg = clients.entry(r.customer_id.clone()).or_default();
        if agg.wallet.is_none() {
            agg.wallet = r.wallet_address.clone().or_else(|| {
                customer_by_id
                    .get(r.customer_id.as_str())
                    .and_then(|c| c.wallet_address.clone())
            });
        }
        if agg.email.is_none() {
            agg.email = r.email.clone().or_else(|| {
                customer_by_id
                    .get(r.customer_id.as_str())
                    .and_then(|c| c.email.clone())
            });
        }
        agg.per_day_cents
            .entry(r.date.clone())
            .and_modify(|v| *v += r.charges_cents)
            .or_insert(r.charges_cents);
        agg.per_day_count
            .entry(r.date.clone())
            .and_modify(|v| *v += r.charges_count)
            .or_insert(r.charges_count);
        agg.total_cents += r.charges_cents;
        agg.total_charges += r.charges_count;
    }

    let mut column_totals_cents: BTreeMap<String, i64> = BTreeMap::new();
    let mut column_totals_count: BTreeMap<String, u64> = BTreeMap::new();
    for agg in clients.values() {
        for (d, v) in &agg.per_day_cents {
            *column_totals_cents.entry(d.clone()).or_insert(0) += v;
        }
        for (d, v) in &agg.per_day_count {
            *column_totals_count.entry(d.clone()).or_insert(0) += v;
        }
    }
    let grand_total_cents: i64 = column_totals_cents.values().sum();
    let grand_total_count: u64 = column_totals_count.values().sum();

    // Sort clients by total_cents descending so the top spenders are at the top.
    let mut client_list: Vec<(&String, &ClientAgg)> = clients.iter().collect();
    client_list.sort_by(|a, b| b.1.total_cents.cmp(&a.1.total_cents).then(a.0.cmp(b.0)));

    let mut html = String::new();
    writeln!(&mut html, "<!DOCTYPE html>").unwrap();
    writeln!(
        &mut html,
        "<html lang=\"en\"><head><meta charset=\"utf-8\">"
    )
    .unwrap();
    writeln!(
        &mut html,
        "<title>Stripe Usage Report — {} to {}</title>",
        html_escape(since_date),
        html_escape(until_date)
    )
    .unwrap();
    writeln!(&mut html, "<style>{STYLES}</style></head><body>").unwrap();
    writeln!(&mut html, "<h1>Stripe Usage Report</h1>").unwrap();
    writeln!(
        &mut html,
        "<p class=\"subtitle\">Last {days} days · {} → {} UTC · {} client(s) · {} across {} charge(s)</p>",
        html_escape(since_date),
        html_escape(until_date),
        client_list.len(),
        html_escape(&cents_to_display(grand_total_cents)),
        grand_total_count,
    )
    .unwrap();

    if client_list.is_empty() {
        writeln!(
            &mut html,
            "<p class=\"empty\">No charges recorded in this window.</p></body></html>"
        )
        .unwrap();
        return html;
    }

    writeln!(&mut html, "<div class=\"scroll\"><table>").unwrap();
    // Header: client | each day | total
    write!(&mut html, "<thead><tr><th class=\"sticky\">Client</th>").unwrap();
    for d in &dates {
        write!(&mut html, "<th>{}</th>", html_escape(d)).unwrap();
    }
    writeln!(&mut html, "<th class=\"total\">Total</th></tr></thead>").unwrap();
    writeln!(&mut html, "<tbody>").unwrap();
    for (cid, agg) in &client_list {
        write!(&mut html, "<tr><td class=\"sticky client\">").unwrap();
        if let Some(w) = &agg.wallet {
            write!(&mut html, "<code>{}</code>", html_escape(w)).unwrap();
        } else {
            write!(&mut html, "<code>{}</code>", html_escape(cid)).unwrap();
        }
        if let Some(e) = &agg.email {
            write!(
                &mut html,
                "<br><span class=\"muted\">{}</span>",
                html_escape(e)
            )
            .unwrap();
        }
        write!(&mut html, "</td>").unwrap();
        for d in &dates {
            let cents = agg.per_day_cents.get(d).copied().unwrap_or(0);
            let count = agg.per_day_count.get(d).copied().unwrap_or(0);
            write_cell(&mut html, cents, count, false);
        }
        write_cell(&mut html, agg.total_cents, agg.total_charges, true);
        writeln!(&mut html, "</tr>").unwrap();
    }
    // Totals row
    write!(
        &mut html,
        "<tr class=\"totals-row\"><td class=\"sticky\">Total</td>"
    )
    .unwrap();
    for d in &dates {
        let cents = column_totals_cents.get(d).copied().unwrap_or(0);
        let count = column_totals_count.get(d).copied().unwrap_or(0);
        write_cell(&mut html, cents, count, false);
    }
    write_cell(&mut html, grand_total_cents, grand_total_count, true);
    writeln!(&mut html, "</tr>").unwrap();
    writeln!(&mut html, "</tbody></table></div></body></html>").unwrap();

    html
}

/// Render one pivot cell showing the dollar amount and the charge count.
/// `total` flips on the bold/highlighted styling used for the totals row/column.
fn write_cell(html: &mut String, cents: i64, count: u64, total: bool) {
    let class = if total { "total" } else { "" };
    if cents == 0 && count == 0 {
        // Keep the zero placeholder visually quiet.
        let z_class = if total { "total zero" } else { "zero" };
        write!(html, "<td class=\"{z_class}\">·</td>").expect("write to String cannot fail");
        return;
    }
    write!(
        html,
        "<td class=\"{class}\">{}<br><span class=\"count\">{}\u{00a0}call{}</span></td>",
        html_escape(&cents_to_display(cents)),
        count,
        if count == 1 { "" } else { "s" },
    )
    .expect("write to String cannot fail");
}

/// Build the inclusive list of `YYYY-MM-DD` dates from `from` to `to`.
/// Falls back to just `[from, to]` if either string is malformed.
fn enumerate_dates(from: &str, to: &str) -> Vec<String> {
    let Some(from_ts) = parse_utc_date(from) else {
        return vec![from.to_string(), to.to_string()];
    };
    let Some(to_ts) = parse_utc_date(to) else {
        return vec![from.to_string(), to.to_string()];
    };
    let mut out = Vec::new();
    let mut ts = from_ts;
    while ts <= to_ts {
        out.push(stripe::unix_to_utc_date(ts));
        ts += 86_400;
    }
    out
}

/// Parse `YYYY-MM-DD` (UTC midnight) → Unix seconds.  `None` on malformed input.
///
/// Inverse of `stripe::unix_to_utc_date` for dates in [1970-01-01, 9999-12-31].
fn parse_utc_date(s: &str) -> Option<i64> {
    let mut parts = s.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Inverse of civil_from_days: days_from_civil.
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = (y_adj - era * 400) as u64; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as u64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146_097 + doe as i64 - 719_468;
    let ts = days * 86_400;
    // Reject impossible dates like 2026-02-31 by round-tripping: if the math
    // silently normalized the components, the formatted timestamp won't match
    // the input string.
    if stripe::unix_to_utc_date(ts) != s {
        return None;
    }
    Some(ts)
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

const STYLES: &str = r#"
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; margin: 2rem; color: #1a1a1a; }
h1 { margin-bottom: 0.25rem; }
.subtitle { color: #555; margin-top: 0; margin-bottom: 1.5rem; }
.scroll { overflow-x: auto; border: 1px solid #e5e5e5; border-radius: 8px; }
table { border-collapse: collapse; font-size: 0.85rem; width: 100%; }
th, td { padding: 0.4rem 0.6rem; border-bottom: 1px solid #eee; text-align: right; white-space: nowrap; }
th { background: #fafafa; font-weight: 600; position: sticky; top: 0; z-index: 1; }
th:first-child, td.sticky { text-align: left; position: sticky; left: 0; background: #fff; z-index: 2; }
thead th:first-child { z-index: 3; background: #fafafa; }
td.client code { font-size: 0.75rem; background: #f3f3f3; padding: 1px 4px; border-radius: 3px; }
td.zero { color: #ccc; text-align: center; }
.count { display: block; color: #888; font-size: 0.7rem; font-variant-numeric: tabular-nums; }
.totals-row .count, td.total .count { color: #555; }
td.total, th.total { font-weight: 600; background: #fafafa; }
.muted { color: #777; font-size: 0.75rem; }
.totals-row { font-weight: 600; background: #fafafa; }
.empty { color: #777; }
"#;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mkrow(date: &str, customer_id: &str, charges_cents: i64) -> ReportRow {
        ReportRow {
            date: date.to_string(),
            customer_id: customer_id.to_string(),
            wallet_address: None,
            email: None,
            charges_count: if charges_cents == 0 { 0 } else { 1 },
            charges_cents,
            credits_cents: 0,
        }
    }

    #[test]
    fn csv_has_header_and_rows() {
        let rows = vec![mkrow("2026-04-21", "cus_a", 5)];
        let csv = render_csv(&rows);
        assert!(csv.starts_with("date,customer_id,wallet_address,email,"));
        assert!(csv.contains("2026-04-21,cus_a,,,1,5,0"));
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        let mut row = mkrow("2026-04-21", "cus_a", 5);
        row.email = Some(r#"a,"b"@example.com"#.to_string());
        let csv = render_csv(&[row]);
        assert!(csv.contains(r#""a,""b""@example.com""#));
    }

    #[test]
    fn csv_escape_neutralizes_formula_injection() {
        // A malicious customer sets their email to a formula. The field must
        // not be executed as a formula when the CSV is opened in Excel.
        for payload in [
            "=cmd|'/C calc'!A0",
            "+1+1",
            "-2+3",
            "@SUM(A1:A9)",
            "\tTAB_PREFIX",
            "\rCR_PREFIX",
        ] {
            let escaped = csv_escape(payload);
            // Result must either start with a single quote (inside a quoted
            // cell or bare) or with `"'` for wrapped cells. Either way, the
            // leading formula-trigger character is defused.
            let stripped = escaped.trim_start_matches('"');
            assert!(
                stripped.starts_with('\''),
                "payload {payload:?} not defused: got {escaped:?}"
            );
        }
    }

    #[test]
    fn csv_escape_leaves_benign_fields_alone() {
        assert_eq!(csv_escape("cus_abc123"), "cus_abc123");
        assert_eq!(csv_escape("0x1234"), "0x1234");
        assert_eq!(csv_escape("alice@example.com"), "alice@example.com");
    }

    #[test]
    fn parse_utc_date_roundtrip() {
        for ts in [0_i64, 1_776_470_400, 1_709_164_800, 946_684_800] {
            let s = stripe::unix_to_utc_date(ts);
            let back = parse_utc_date(&s).unwrap();
            assert_eq!(back, ts, "roundtrip failed for ts={ts} ({s})");
        }
    }

    #[test]
    fn parse_utc_date_bad_input() {
        assert!(parse_utc_date("not-a-date").is_none());
        assert!(parse_utc_date("2026-13-01").is_none());
        assert!(parse_utc_date("2026-04-00").is_none());
        assert!(parse_utc_date("2026-04-21-01").is_none());
    }

    #[test]
    fn parse_utc_date_rejects_impossible_calendar_days() {
        // These would silently normalize without round-trip validation (e.g.
        // Feb 31 → Mar 3), producing a wrong date range in the report.
        assert!(parse_utc_date("2026-02-31").is_none());
        assert!(parse_utc_date("2026-02-30").is_none());
        assert!(parse_utc_date("2025-02-29").is_none()); // 2025 is not leap
        assert!(parse_utc_date("2026-04-31").is_none()); // April has 30 days
        // Actual leap day in a leap year must still parse.
        assert!(parse_utc_date("2024-02-29").is_some());
    }

    #[test]
    fn csv_escape_quotes_embedded_carriage_return() {
        // Bare \r inside a field must be quoted so CSV consumers that treat
        // \r as a record separator don't split the row.
        let escaped = csv_escape("alice\rbob@example.com");
        assert!(
            escaped.starts_with('"') && escaped.ends_with('"'),
            "expected quoted field, got {escaped:?}"
        );
    }

    #[test]
    fn enumerate_dates_inclusive() {
        let dates = enumerate_dates("2026-04-20", "2026-04-22");
        assert_eq!(dates, vec!["2026-04-20", "2026-04-21", "2026-04-22"]);
    }

    #[test]
    fn enumerate_dates_single_day() {
        let dates = enumerate_dates("2026-04-20", "2026-04-20");
        assert_eq!(dates, vec!["2026-04-20"]);
    }

    #[test]
    fn html_empty_rows_still_renders() {
        let html = render_html(&[], &[], "2026-04-01", "2026-04-02", 2);
        assert!(html.contains("No charges recorded"));
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn html_contains_pivot_table() {
        let rows = vec![
            mkrow("2026-04-21", "cus_a", 500),
            mkrow("2026-04-22", "cus_a", 300),
            mkrow("2026-04-22", "cus_b", 100),
        ];
        let html = render_html(&rows, &[], "2026-04-21", "2026-04-22", 2);
        assert!(html.contains("cus_a"));
        assert!(html.contains("cus_b"));
        // Per-day amounts.
        assert!(html.contains("$5.00"));
        assert!(html.contains("$3.00"));
        assert!(html.contains("$1.00"));
        // Per-day counts (1 call each in the test data).
        assert!(html.contains("1\u{00a0}call</span>"));
        // Per-client totals: cus_a = $8.00 / 2 calls; cus_b = $1.00 / 1 call.
        assert!(html.contains("$8.00"));
        // Column total for 2026-04-22: $4.00 across 2 calls.
        assert!(html.contains("$4.00"));
        assert!(html.contains("2\u{00a0}calls</span>"));
        // Subtitle reports the grand-total count.
        assert!(html.contains("across 3 charge(s)"));
    }

    #[test]
    fn html_zero_cells_render_placeholder() {
        // cus_a has activity only on 2026-04-21; the 2026-04-22 cell must be the
        // muted placeholder (no "0 calls" line).
        let rows = vec![mkrow("2026-04-21", "cus_a", 100)];
        let html = render_html(&rows, &[], "2026-04-21", "2026-04-22", 2);
        assert!(html.contains("class=\"zero\">·</td>"));
        assert!(!html.contains("0\u{00a0}call"));
    }

    #[test]
    fn html_escapes_injected_content() {
        let mut row = mkrow("2026-04-21", "cus_a", 5);
        row.wallet_address = Some("<script>alert(1)</script>".to_string());
        let html = render_html(&[row], &[], "2026-04-21", "2026-04-21", 1);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
