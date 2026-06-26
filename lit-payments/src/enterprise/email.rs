//! Review email for the monthly enterprise invoice. Sent to the account's
//! `notify_email` so a human can sanity-check the numbers and click Send on the
//! draft invoice in Stripe (v1, before we drop the human-in-the-loop step).

use anyhow::Result;
use lit_billing_core::format::cents_to_display;

use super::types::{EnterpriseAccount, EnterpriseInvoice};
use crate::mail::Mailer;

/// Send the breakdown email with a link to the draft invoice.
pub async fn send_review_email(
    mailer: &Mailer,
    account: &EnterpriseAccount,
    inv: &EnterpriseInvoice,
    invoice_url: &str,
) -> Result<()> {
    let subject = format!(
        "[Lit] {} — draft invoice {} ready ({})",
        account.name,
        inv.period_key,
        cents_to_display(inv.total_cents),
    );

    let committed = cents_to_display(inv.committed_fee_cents);
    let overage = cents_to_display(inv.overage_cents);
    let total = cents_to_display(inv.total_cents);

    // Metering sanity flag surfaced to the reviewer. 0 units for an active
    // enterprise account is unusual and usually means an external credit hit the
    // payer (which understates overage) or usage genuinely stopped — either way,
    // worth a look before sending.
    let caution = (inv.consumed_units == 0).then_some(
        "0 units recorded this cycle — unusual for an active account. Verify the payer's Stripe \
         balance and that no external credit (admin grant / LITKEY top-up) hit the payer account, \
         which would understate overage.",
    );
    let text_caution = caution
        .map(|c| format!("\n⚠ HEADS UP: {c}\n"))
        .unwrap_or_default();
    let html_caution = caution
        .map(|c| {
            format!(
                "<p style=\"background:#fff3cd;border:1px solid #ffe69c;padding:8px;\">\
                 ⚠ <b>Heads up:</b> {c}</p>"
            )
        })
        .unwrap_or_default();

    let text = format!(
        "Draft invoice for {name} is ready for review.\n\
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
         Review and send the draft invoice here:\n\
         {url}\n\
         \n\
         (Invoice goes to {invoice_cust}; credits/usage are on payer {payer_cust}.)\n\
         The payer's credit buffer has been topped back up to ${target}.\n",
        name = account.name,
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
        url = invoice_url,
        invoice_cust = account.invoice_customer_id,
        payer_cust = account.payer_customer_id,
        target = account.target_credit_cents / 100,
        text_caution = text_caution,
    );

    let html = format!(
        "<h2>Draft invoice for {name} is ready for review</h2>\
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
         <p><a href=\"{url}\">Review &amp; send the draft invoice in Stripe →</a></p>\
         <p style=\"color:#666;font-size:12px\">Invoice goes to {invoice_cust}; credits/usage \
         are on payer {payer_cust}. The payer's credit buffer has been topped back up to \
         ${target}.</p>",
        name = account.name,
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
