//! Notification emails for the monthly enterprise invoice, sent to the
//! account's `notify_email`.
//!
//! Two variants: a **review** email when the invoice is left as a draft (a
//! human sanity-checks the numbers and clicks Send in Stripe), and a **sent**
//! FYI when `auto_send` finalized + sent the invoice automatically.

use anyhow::Result;
use lit_billing_core::format::cents_to_display;

use super::types::{EnterpriseAccount, EnterpriseInvoice};
use crate::mail::Mailer;

/// Send the review breakdown with a link to the draft invoice (manual-send
/// accounts, and auto_send accounts whose cycle was held as anomalous).
pub async fn send_review_email(
    mailer: &Mailer,
    account: &EnterpriseAccount,
    inv: &EnterpriseInvoice,
    invoice_url: &str,
) -> Result<()> {
    send_invoice_email(mailer, account, inv, invoice_url, false).await
}

/// Send the FYI breakdown after `auto_send` finalized + sent the invoice.
pub async fn send_sent_email(
    mailer: &Mailer,
    account: &EnterpriseAccount,
    inv: &EnterpriseInvoice,
    invoice_url: &str,
) -> Result<()> {
    send_invoice_email(mailer, account, inv, invoice_url, true).await
}

async fn send_invoice_email(
    mailer: &Mailer,
    account: &EnterpriseAccount,
    inv: &EnterpriseInvoice,
    invoice_url: &str,
    sent: bool,
) -> Result<()> {
    let subject = if sent {
        format!(
            "[Lit] {} — invoice {} sent ({})",
            account.name,
            inv.period_key,
            cents_to_display(inv.total_cents),
        )
    } else {
        format!(
            "[Lit] {} — draft invoice {} ready ({})",
            account.name,
            inv.period_key,
            cents_to_display(inv.total_cents),
        )
    };

    let committed = cents_to_display(inv.committed_fee_cents);
    let overage = cents_to_display(inv.overage_cents);
    let total = cents_to_display(inv.total_cents);

    // Metering sanity flags surfaced to the reviewer, mirroring
    // `calc::hold_for_review`. Anomalous cycles are always held as a draft, so
    // neither flag ever appears on the sent variant.
    let mut caution = if inv.consumed_units == 0 {
        Some(
            "0 units recorded this cycle — unusual for an active account. Verify the payer's \
             Stripe balance and that no external credit (admin grant / LITKEY top-up) hit the \
             payer account, which would understate overage."
                .to_string(),
        )
    } else if inv.consumed_units >= account.target_credit_cents {
        Some(format!(
            "consumed reading ({} units) is at/above the full ${} buffer target — the payer's \
             balance went to zero or positive during the cycle (buffer exhausted, or an external \
             debit hit the payer), so the overage figure is unreliable. Verify the payer's Stripe \
             balance history before sending.",
            inv.consumed_units,
            account.target_credit_cents / 100,
        ))
    } else {
        None
    };
    if account.auto_send
        && let Some(c) = caution.as_mut()
    {
        c.push_str(
            " Auto-send is enabled for this account but this cycle was HELD as a draft pending \
             review.",
        );
    }
    let text_caution = caution
        .as_ref()
        .map(|c| format!("\n⚠ HEADS UP: {c}\n"))
        .unwrap_or_default();
    let html_caution = caution
        .as_ref()
        .map(|c| {
            format!(
                "<p style=\"background:#fff3cd;border:1px solid #ffe69c;padding:8px;\">\
                 ⚠ <b>Heads up:</b> {c}</p>"
            )
        })
        .unwrap_or_default();

    let (text_lead, text_cta) = if sent {
        (
            format!(
                "Invoice for {} has been finalized and sent to the customer (net-30)",
                account.name
            ),
            "View the invoice here:",
        )
    } else {
        (
            format!("Draft invoice for {} is ready for review", account.name),
            "Review and send the draft invoice here:",
        )
    };

    let text = format!(
        "{lead}.\n\
         {text_caution}\
         \n\
         Period (issued): {period}\n\
         Committed cycle (advance): {committed_period}\n\
         Overage window (arrears): {p_start} -> {p_end}\n\
         \n\
         Usage this cycle:\n\
         - Consumed:  {consumed} units\n\
         - Included:  {included} units\n\
         - Overage:   {over} units\n\
         \n\
         Charges:\n\
         - Committed monthly fee: {committed}\n\
         - Overage @ $0.0025/unit: {overage}\n\
         - TOTAL: {total}\n\
         \n\
         {cta}\n\
         {url}\n\
         \n\
         (Invoice goes to {invoice_cust}; credits/usage are on payer {payer_cust}.)\n\
         The payer's credit buffer has been topped back up to ${target}.\n",
        lead = text_lead,
        period = inv.period_key,
        committed_period = inv.committed_period,
        p_start = inv.period_start,
        p_end = inv.period_end,
        consumed = inv.consumed_units,
        included = inv.included_units,
        over = inv.overage_units,
        committed = committed,
        overage = overage,
        total = total,
        cta = text_cta,
        url = invoice_url,
        invoice_cust = account.invoice_customer_id,
        payer_cust = account.payer_customer_id,
        target = account.target_credit_cents / 100,
        text_caution = text_caution,
    );

    let (html_heading, html_cta) = if sent {
        (
            format!(
                "Invoice for {} has been finalized and sent to the customer",
                account.name
            ),
            "View the invoice in Stripe →",
        )
    } else {
        (
            format!("Draft invoice for {} is ready for review", account.name),
            "Review &amp; send the draft invoice in Stripe →",
        )
    };

    let html = format!(
        "<h2>{heading}</h2>\
         {html_caution}\
         <table cellpadding=\"4\">\
           <tr><td><b>Period (issued)</b></td><td>{period}</td></tr>\
           <tr><td><b>Committed cycle (advance)</b></td><td>{committed_period}</td></tr>\
           <tr><td><b>Overage window (arrears)</b></td><td>{p_start} → {p_end}</td></tr>\
         </table>\
         <h3>Usage this cycle</h3>\
         <table cellpadding=\"4\">\
           <tr><td>Consumed</td><td align=\"right\">{consumed} units</td></tr>\
           <tr><td>Included</td><td align=\"right\">{included} units</td></tr>\
           <tr><td>Overage</td><td align=\"right\">{over} units</td></tr>\
         </table>\
         <h3>Charges</h3>\
         <table cellpadding=\"4\">\
           <tr><td>Committed monthly fee</td><td align=\"right\">{committed}</td></tr>\
           <tr><td>Overage @ $0.0025/unit</td><td align=\"right\">{overage}</td></tr>\
           <tr><td><b>Total</b></td><td align=\"right\"><b>{total}</b></td></tr>\
         </table>\
         <p><a href=\"{url}\">{cta}</a></p>\
         <p style=\"color:#666;font-size:12px\">Invoice goes to {invoice_cust}; credits/usage \
         are on payer {payer_cust}. The payer's credit buffer has been topped back up to \
         ${target}.</p>",
        heading = html_heading,
        period = inv.period_key,
        committed_period = inv.committed_period,
        p_start = inv.period_start,
        p_end = inv.period_end,
        consumed = inv.consumed_units,
        included = inv.included_units,
        over = inv.overage_units,
        committed = committed,
        overage = overage,
        total = total,
        cta = html_cta,
        url = invoice_url,
        invoice_cust = account.invoice_customer_id,
        payer_cust = account.payer_customer_id,
        target = account.target_credit_cents / 100,
        html_caution = html_caution,
    );

    mailer
        .send(&account.notify_email, &subject, &html, &text)
        .await
}
