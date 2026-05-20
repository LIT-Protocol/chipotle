//! Stripe customer / balance-transaction listing + aggregation.
//!
//! Powers the `stripe_report` binary. Not used by the running API server.

use std::collections::BTreeMap;

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

/// One customer balance transaction as returned by [`list_balance_transactions_since`].
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
}

/// One row of the per-day-per-customer usage report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    pub date: String,
    pub customer_id: String,
    pub wallet_address: Option<String>,
    pub email: Option<String>,
    /// Number of positive-amount balance transactions (charges) on this day.
    pub charges_count: u64,
    /// Sum of positive amounts in cents (debits to the customer's credit balance).
    pub charges_cents: i64,
    /// Sum of absolute value of negative amounts in cents (credits / top-ups).
    pub credits_cents: i64,
}

/// Page over `GET /v1/customers` and return every customer, 100 at a time.
pub async fn list_all_customers(client: &StripeClient) -> Result<Vec<ReportCustomer>> {
    let mut out = Vec::new();
    let mut starting_after: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> = vec![("limit", "100")];
        if let Some(cursor) = starting_after.as_deref() {
            query.push(("starting_after", cursor));
        }
        let resp = client.get("customers", &query).await?;
        let data = resp
            .body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        if data.is_empty() {
            break;
        }
        for c in &data {
            let Some(id) = c.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let wallet = c
                .get("metadata")
                .and_then(|m| m.get("wallet_address"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string());
            let email = c
                .get("email")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            out.push(ReportCustomer {
                id: id.to_string(),
                wallet_address: wallet,
                email,
            });
        }
        let has_more = resp
            .body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !has_more {
            break;
        }
        // Cursor must come from the raw response's last item, not from `out`.
        // If every item in a page failed the id guard above, `out.last()` would
        // not advance and we would re-request the same page forever.
        let next_cursor = data
            .last()
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match next_cursor {
            Some(c) => starting_after = Some(c),
            None => break,
        }
    }
    Ok(out)
}

/// Fetch all customer balance transactions created at or after `since_unix`
/// (seconds since epoch), paginating 100 at a time.
pub async fn list_balance_transactions_since(
    client: &StripeClient,
    customer_id: &str,
    since_unix: i64,
) -> Result<Vec<ReportBalanceTx>> {
    let path = format!("customers/{customer_id}/balance_transactions");
    let since_str = since_unix.to_string();
    let mut out = Vec::new();
    let mut starting_after: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> =
            vec![("limit", "100"), ("created[gte]", since_str.as_str())];
        if let Some(cursor) = starting_after.as_deref() {
            query.push(("starting_after", cursor));
        }
        let resp = client.get(&path, &query).await?;
        let data = resp
            .body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        if data.is_empty() {
            break;
        }
        for tx in &data {
            let Some(id) = tx.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            // Don't silently default missing amount/created to 0 — that would
            // bucket malformed transactions into 1970-01-01 with $0 and skew
            // the report. Skip with a warning instead.
            let Some(amount) = tx.get("amount").and_then(|v| v.as_i64()) else {
                tracing::warn!(
                    "stripe_report: skipping tx {id} for customer {customer_id}: missing/invalid 'amount' field"
                );
                continue;
            };
            let Some(created) = tx.get("created").and_then(|v| v.as_i64()) else {
                tracing::warn!(
                    "stripe_report: skipping tx {id} for customer {customer_id}: missing/invalid 'created' field"
                );
                continue;
            };
            let description = tx
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.push(ReportBalanceTx {
                id: id.to_string(),
                customer_id: customer_id.to_string(),
                amount,
                created,
                description,
            });
        }
        let has_more = resp
            .body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !has_more {
            break;
        }
        // See note in list_all_customers: derive the cursor from the raw response,
        // not from `out`, so a fully-filtered page can't stall the loop.
        let next_cursor = data
            .last()
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match next_cursor {
            Some(c) => starting_after = Some(c),
            None => break,
        }
    }
    Ok(out)
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
    for tx in transactions {
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
                charges_cents: 0,
                credits_cents: 0,
            }
        });
        if tx.amount > 0 {
            row.charges_count += 1;
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
}
