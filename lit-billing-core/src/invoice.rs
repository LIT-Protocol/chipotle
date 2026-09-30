//! Stripe invoicing primitives.
//!
//! Used by the enterprise committed-use billing job in `lit-payments`: create a
//! *draft* invoice on the invoice customer, attach line items, and — for
//! `auto_send` accounts — finalize + send it.
//!
//! Invoices are created with `collection_method = send_invoice` and
//! `days_until_due = 30` (net-30), and `auto_advance = false` so they stay as a
//! reviewable **draft** until explicitly finalized — by a human in the Stripe
//! dashboard, or by [`finalize_and_send`] for `auto_send` accounts. All POSTs
//! take an idempotency key so retries can't create duplicate invoices or line
//! items.

use anyhow::Result;

use crate::client::StripeClient;

/// Create a **draft** invoice for `customer_id` (net-30, send-invoice
/// collection). Returns the Stripe invoice id (`in_…`).
///
/// `pending_invoice_items_behavior=exclude` means this invoice does NOT sweep in
/// the customer's other floating invoice items — only the items we explicitly
/// attach via [`add_invoice_item`] with this invoice id land on it.
pub async fn create_draft_invoice(
    client: &StripeClient,
    customer_id: &str,
    days_until_due: i64,
    description: &str,
    idempotency_key: &str,
) -> Result<String> {
    let days_str = days_until_due.to_string();
    let params = [
        ("customer", customer_id),
        ("collection_method", "send_invoice"),
        ("days_until_due", days_str.as_str()),
        // Keep it a reviewable draft; do not auto-finalize on a schedule.
        ("auto_advance", "false"),
        ("pending_invoice_items_behavior", "exclude"),
        ("description", description),
    ];
    let resp = client
        .post_with_idempotency("invoices", &params, idempotency_key)
        .await?;
    let id = resp
        .body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe: missing invoice id"))?
        .to_string();
    Ok(id)
}

/// Attach a single line item (`amount_cents`, USD) to draft `invoice_id` for
/// `customer_id`. Returns the invoice-item id (`ii_…`). Stripe rejects a
/// zero-amount item, so callers must skip items whose amount is 0.
pub async fn add_invoice_item(
    client: &StripeClient,
    customer_id: &str,
    invoice_id: &str,
    amount_cents: i64,
    description: &str,
    idempotency_key: &str,
) -> Result<String> {
    let amount_str = amount_cents.to_string();
    let params = [
        ("customer", customer_id),
        ("invoice", invoice_id),
        ("amount", amount_str.as_str()),
        ("currency", "usd"),
        ("description", description),
    ];
    let resp = client
        .post_with_idempotency("invoiceitems", &params, idempotency_key)
        .await?;
    let id = resp
        .body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe: missing invoiceitem id"))?
        .to_string();
    Ok(id)
}

/// Finalize a draft invoice and email it to the customer (net-30). Used by the
/// billing job for `auto_send` accounts; manual-send accounts finalize from the
/// dashboard after review instead.
///
/// The explicit `invoices/{id}/send` call is what guarantees the customer email:
/// finalization alone only emails when the Stripe account's "email finalized
/// invoices" dashboard setting happens to be on, which this code doesn't control.
/// The two POSTs take separate idempotency keys so a retry that finds the
/// invoice already finalized can still be deduped on the send step.
pub async fn finalize_and_send(
    client: &StripeClient,
    invoice_id: &str,
    finalize_idempotency_key: &str,
    send_idempotency_key: &str,
) -> Result<()> {
    let path = format!("invoices/{invoice_id}/finalize");
    client
        .post_with_idempotency(&path, &[("auto_advance", "true")], finalize_idempotency_key)
        .await?;
    let path = format!("invoices/{invoice_id}/send");
    client
        .post_with_idempotency(&path, &[], send_idempotency_key)
        .await?;
    Ok(())
}
