//! Stripe customer / balance-transaction listing + aggregation.
//!
//! Powers the `stripe_report` binary. Not used by the running API server.

use std::collections::{BTreeMap, HashSet};

use anyhow::Result;

use crate::client::StripeClient;
use crate::format::unix_to_utc_date;

/// One Stripe customer as returned by [`list_all_customers`].
#[derive(Debug, Clone)]
pub struct ReportCustomer {
    pub id: String,
    pub wallet_address: Option<String>,
    pub email: Option<String>,
}

/// One customer balance transaction as returned by [`list_balance_transactions_in_window`].
///
/// `created` is a Unix timestamp in seconds. `amount` is in the currency's
/// minor unit (cents for USD): positive = charge (debit to the customer's
/// credit balance), negative = credit (top-up).
#[derive(Debug, Clone)]
pub struct ReportBalanceTx {
    pub id: String,
    pub customer_id: String,
    pub amount: i64,
    pub created: i64,
    pub description: String,
    pub transaction_type: String,
    pub reason: Option<String>,
    pub request_id: Option<String>,
}

/// One row of the per-day-per-customer usage report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    pub date: String,
    pub customer_id: String,
    pub wallet_address: Option<String>,
    pub email: Option<String>,
    /// Number of identified usage charge records, not API requests.
    pub charges_count: u64,
    /// Distinct (customer, request ID) pairs observed in this report window.
    /// Assigned to the first charge date in the window, not necessarily request start.
    pub identified_requests_count: u64,
    /// Usage charge records lacking request IDs; cannot be converted to request counts.
    pub unattributed_charges_count: u64,
    /// Sum of identified USD usage charges in cents.
    pub charges_cents: i64,
    /// Sum of absolute value of negative amounts in cents (credits / top-ups).
    pub credits_cents: i64,
}

/// The last N completed UTC days, with an exclusive end at today's midnight.
pub fn completed_days_window(now: i64, days: u32) -> Result<(i64, i64)> {
    anyhow::ensure!(
        (1..=3660).contains(&days),
        "days must be between 1 and 3660"
    );
    let until = now - now.rem_euclid(86_400);
    Ok((until - i64::from(days) * 86_400, until))
}

#[derive(serde::Deserialize)]
struct Page {
    data: Vec<serde_json::Value>,
    has_more: bool,
}

fn parse_page(body: serde_json::Value) -> Result<Page> {
    // Do not include response bodies or customer identifiers in errors.
    let page: Page =
        serde_json::from_value(body).map_err(|_| anyhow::anyhow!("invalid Stripe report page"))?;
    anyhow::ensure!(
        !page.has_more || !page.data.is_empty(),
        "empty Stripe page with more data"
    );
    Ok(page)
}

fn required_string<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing or invalid report field: {field}"))
}

fn next_cursor(page: &Page, previous: Option<&str>) -> Result<Option<String>> {
    if !page.has_more {
        return Ok(None);
    }
    let last = page
        .data
        .last()
        .ok_or_else(|| anyhow::anyhow!("empty Stripe page"))?;
    let id = required_string(last, "id")?;
    anyhow::ensure!(Some(id) != previous, "Stripe pagination did not advance");
    Ok(Some(id.to_owned()))
}

/// Page over every customer. Malformed pages fail instead of producing partial reports.
pub async fn list_all_customers(client: &StripeClient) -> Result<Vec<ReportCustomer>> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("limit", "100")];
        if let Some(c) = cursor.as_deref() {
            query.push(("starting_after", c));
        }
        let resp = client.get("customers", &query).await?;
        anyhow::ensure!(resp.status.is_success(), "Stripe customer listing failed");
        let page = parse_page(resp.body)?;
        for c in &page.data {
            out.push(ReportCustomer {
                id: required_string(c, "id")?.to_owned(),
                wallet_address: c
                    .pointer("/metadata/wallet_address")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned),
                email: c
                    .get("email")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            });
        }
        cursor = next_cursor(&page, cursor.as_deref())?;
        if cursor.is_none() {
            break;
        }
    }
    Ok(out)
}

fn parse_transaction(
    tx: &serde_json::Value,
    customer_id: &str,
    since: i64,
    until: i64,
) -> Result<Option<ReportBalanceTx>> {
    let created = tx
        .get("created")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("invalid transaction timestamp"))?;
    // Enforce both bounds locally as well as in the API query.
    if created < since || created >= until {
        return Ok(None);
    }
    anyhow::ensure!(
        required_string(tx, "currency")? == "usd",
        "non-USD transaction in report window"
    );
    Ok(Some(ReportBalanceTx {
        id: required_string(tx, "id")?.to_owned(),
        customer_id: customer_id.to_owned(),
        amount: tx
            .get("amount")
            .and_then(|v| v.as_i64())
            .filter(|v| *v != i64::MIN)
            .ok_or_else(|| anyhow::anyhow!("invalid transaction amount"))?,
        created,
        description: tx
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        transaction_type: required_string(tx, "type")?.to_owned(),
        request_id: tx
            .pointer("/metadata/request_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned),
        reason: tx
            .pointer("/metadata/reason")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    }))
}

/// Fetch transactions in [since_unix, until_unix), paginating 100 at a time.
pub async fn list_balance_transactions_in_window(
    client: &StripeClient,
    customer_id: &str,
    since_unix: i64,
    until_unix: i64,
) -> Result<Vec<ReportBalanceTx>> {
    anyhow::ensure!(since_unix < until_unix, "invalid report window");
    let path = format!("customers/{customer_id}/balance_transactions");
    let since = since_unix.to_string();
    let until = until_unix.to_string();
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![
            ("limit", "100"),
            ("created[gte]", since.as_str()),
            ("created[lt]", until.as_str()),
        ];
        if let Some(c) = cursor.as_deref() {
            query.push(("starting_after", c));
        }
        let resp = client.get(&path, &query).await?;
        anyhow::ensure!(
            resp.status.is_success(),
            "Stripe transaction listing failed"
        );
        let page = parse_page(resp.body)?;
        for tx in &page.data {
            if let Some(tx) = parse_transaction(tx, customer_id, since_unix, until_unix)? {
                out.push(tx);
            }
        }
        cursor = next_cursor(&page, cursor.as_deref())?;
        if cursor.is_none() {
            break;
        }
    }
    Ok(out)
}

fn is_usage_charge(tx: &ReportBalanceTx) -> bool {
    if tx.amount <= 0 || tx.transaction_type != "adjustment" {
        return false;
    }
    match tx.reason.as_deref() {
        Some("management" | "lit_action") => true,
        // Legacy charges predate reason metadata. Only accept known billing labels.
        None => {
            tx.description == "Configuration change"
                || tx.description == "Lit Action execution"
                || tx
                    .description
                    .strip_prefix("Lit Action ")
                    .is_some_and(|cid| !cid.trim().is_empty())
        }
        Some(_) => false,
    }
}

/// Aggregate a flat list of balance transactions into one row per
/// (date, customer_id) pair.
///
/// `customers` provides wallet/email lookup by customer id. Transactions whose
/// `customer_id` is not present in `customers` are still bucketed but have
/// `wallet_address` and `email` set to `None`.
pub fn aggregate_report_rows(
    customers: &[ReportCustomer],
    transactions: &[ReportBalanceTx],
) -> Vec<ReportRow> {
    let customer_by_id: std::collections::HashMap<&str, &ReportCustomer> =
        customers.iter().map(|c| (c.id.as_str(), c)).collect();
    // BTreeMap so output is sorted by (date, customer_id) deterministically.
    let mut buckets: BTreeMap<(String, String), ReportRow> = BTreeMap::new();
    let mut seen_requests = HashSet::new();
    let mut ordered: Vec<_> = transactions.iter().collect();
    ordered.sort_by_key(|tx| tx.created);
    for tx in ordered {
        if tx.amount >= 0 && !is_usage_charge(tx) {
            continue;
        }
        let date = unix_to_utc_date(tx.created);
        let key = (date.clone(), tx.customer_id.clone());
        let row = buckets.entry(key).or_insert_with(|| {
            let cust = customer_by_id.get(tx.customer_id.as_str()).copied();
            ReportRow {
                date: date.clone(),
                customer_id: tx.customer_id.clone(),
                wallet_address: cust.and_then(|c| c.wallet_address.clone()),
                email: cust.and_then(|c| c.email.clone()),
                charges_count: 0,
                identified_requests_count: 0,
                unattributed_charges_count: 0,
                charges_cents: 0,
                credits_cents: 0,
            }
        });
        if tx.amount > 0 {
            row.charges_count += 1;
            match tx.request_id.as_deref().filter(|id| !id.trim().is_empty()) {
                Some(id) => {
                    if seen_requests.insert((tx.customer_id.as_str(), id)) {
                        row.identified_requests_count += 1;
                    }
                }
                None => row.unattributed_charges_count += 1,
            }
            row.charges_cents += tx.amount;
        } else if tx.amount < 0 {
            row.credits_cents += -tx.amount;
        }
    }
    buckets.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(customer_id: &str, amount: i64, created: i64) -> ReportBalanceTx {
        ReportBalanceTx {
            id: format!("tx_{customer_id}_{created}_{amount}"),
            customer_id: customer_id.to_string(),
            amount,
            created,
            description: String::new(),
            transaction_type: "adjustment".to_string(),
            reason: Some("management".to_string()),
            request_id: None,
        }
    }

    #[test]
    fn aggregate_report_rows_empty() {
        assert!(aggregate_report_rows(&[], &[]).is_empty());
    }

    #[test]
    fn aggregate_report_rows_buckets_by_day_and_customer() {
        let customers = vec![
            ReportCustomer {
                id: "cus_a".to_string(),
                wallet_address: Some("0xA".to_string()),
                email: None,
            },
            ReportCustomer {
                id: "cus_b".to_string(),
                wallet_address: Some("0xB".to_string()),
                email: Some("b@example.com".to_string()),
            },
        ];
        let day1 = 1_776_729_600; // 2026-04-21 00:00:00 UTC
        let day2 = day1 + 86_400; // 2026-04-22 00:00:00 UTC
        let txs = vec![
            tx("cus_a", 1, day1 + 10),
            tx("cus_a", 1, day1 + 20),
            tx("cus_a", 1, day2 + 5),
            tx("cus_b", 5, day1 + 1),
            tx("cus_b", -500, day1 + 2), // top-up credit
        ];
        let rows = aggregate_report_rows(&customers, &txs);
        assert_eq!(rows.len(), 3);
        // Sorted by (date, customer_id) ascending.
        assert_eq!(rows[0].date, "2026-04-21");
        assert_eq!(rows[0].customer_id, "cus_a");
        assert_eq!(rows[0].charges_count, 2);
        assert_eq!(rows[0].charges_cents, 2);
        assert_eq!(rows[0].credits_cents, 0);
        assert_eq!(rows[0].wallet_address.as_deref(), Some("0xA"));
        assert_eq!(rows[1].date, "2026-04-21");
        assert_eq!(rows[1].customer_id, "cus_b");
        assert_eq!(rows[1].charges_count, 1);
        assert_eq!(rows[1].charges_cents, 5);
        assert_eq!(rows[1].credits_cents, 500);
        assert_eq!(rows[1].email.as_deref(), Some("b@example.com"));
        assert_eq!(rows[2].date, "2026-04-22");
        assert_eq!(rows[2].customer_id, "cus_a");
        assert_eq!(rows[2].charges_count, 1);
    }

    #[test]
    fn aggregate_report_rows_unknown_customer_still_bucketed() {
        let day1 = 1_776_729_600; // 2026-04-21 00:00:00 UTC
        let txs = vec![tx("cus_unknown", 3, day1)];
        let rows = aggregate_report_rows(&[], &txs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].customer_id, "cus_unknown");
        assert_eq!(rows[0].wallet_address, None);
        assert_eq!(rows[0].email, None);
        assert_eq!(rows[0].charges_cents, 3);
    }
    #[test]
    fn completed_window_excludes_today_and_is_stable_throughout_day() {
        let midnight = 1_776_729_600;
        for offset in [0, 28_800, 86_399] {
            assert_eq!(
                completed_days_window(midnight + offset, 7).unwrap(),
                (midnight - 7 * 86_400, midnight)
            );
        }
        assert_eq!(
            completed_days_window(midnight, 1).unwrap(),
            (midnight - 86_400, midnight)
        );
        assert!(completed_days_window(midnight, 0).is_err());
        assert!(completed_days_window(midnight, u32::MAX).is_err());
    }

    fn raw_tx(created: i64) -> serde_json::Value {
        serde_json::json!({"id":"cbtxn_test", "created":created, "amount":1, "currency":"usd",
            "type":"adjustment", "metadata":{"reason":"lit_action"}})
    }

    #[test]
    fn transaction_bounds_are_inclusive_start_exclusive_end() {
        for (created, included) in [(99, false), (100, true), (199, true), (200, false)] {
            assert_eq!(
                parse_transaction(&raw_tx(created), "cus_test", 100, 200)
                    .unwrap()
                    .is_some(),
                included
            );
        }
    }

    #[test]
    fn invalid_or_non_usd_transactions_fail_closed() {
        for field in ["id", "created", "amount", "currency", "type"] {
            let mut raw = raw_tx(100);
            raw.as_object_mut().unwrap().remove(field);
            assert!(parse_transaction(&raw, "cus_test", 100, 200).is_err());
        }
        let mut raw = raw_tx(100);
        raw["currency"] = serde_json::json!("eur");
        assert!(parse_transaction(&raw, "cus_test", 100, 200).is_err());
    }

    #[test]
    fn invalid_pages_and_stalled_cursors_fail_closed() {
        assert!(parse_page(serde_json::json!({})).is_err());
        assert!(parse_page(serde_json::json!({"data":[], "has_more":true})).is_err());
        let page = parse_page(serde_json::json!({"data":[{"id":"a"}], "has_more":true})).unwrap();
        assert_eq!(next_cursor(&page, None).unwrap().as_deref(), Some("a"));
        assert!(next_cursor(&page, Some("a")).is_err());
    }

    #[test]
    fn only_identified_adjustments_count_as_usage() {
        let mut charge = tx("cus_test", 1, 1_776_729_600);
        let mut invoice = charge.clone();
        invoice.transaction_type = "applied_to_invoice".into();
        let mut manual = charge.clone();
        manual.reason = None;
        manual.description = "Manual adjustment".into();
        let mut other = charge.clone();
        other.reason = Some("other".into());
        other.description = "Configuration change".into();
        let mut legacy = charge.clone();
        legacy.reason = None;
        legacy.description = "Lit Action execution".into();
        charge.reason = Some("lit_action".into());
        // Two charge records may belong to one execution; retain both amounts
        // and explicitly present the count as charges, never requests.
        let rows = aggregate_report_rows(&[], &[charge, invoice, manual, other, legacy]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].charges_count, 2);
        assert_eq!(rows[0].charges_cents, 2);
    }
    #[test]
    fn request_counts_deduplicate_across_days_but_not_customers() {
        let day = 1_776_729_600;
        let mut first = tx("cus_a", 2, day);
        first.request_id = Some("req_one".into());
        let mut later = first.clone();
        later.created += 86_400;
        let mut other_customer = first.clone();
        other_customer.customer_id = "cus_b".into();
        let unknown = tx("cus_a", 3, day);
        let rows = aggregate_report_rows(&[], &[later, unknown, other_customer, first]);
        assert_eq!(rows[0].charges_count, 2);
        assert_eq!(rows[0].identified_requests_count, 1);
        assert_eq!(rows[0].unattributed_charges_count, 1);
        assert_eq!(rows[1].identified_requests_count, 1);
        assert_eq!(rows[2].charges_count, 1);
        assert_eq!(rows[2].identified_requests_count, 0);
        assert_eq!(rows.iter().map(|r| r.charges_cents).sum::<i64>(), 9);
    }
}
