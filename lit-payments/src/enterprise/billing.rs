//! Enterprise committed-use billing job.
//!
//! One background task, spawned at startup. Each tick, for every active
//! `enterprise_accounts` row:
//!   1. Establish the credit buffer once (baseline grant to target).
//!   2. Determine the current anchor period.
//!   3. If no invoice exists for it yet, snapshot consumption from the payer's
//!      live Stripe balance, create a DRAFT invoice on the invoice account,
//!      regrant the payer buffer back to target, and then either finalize +
//!      send the invoice automatically (`auto_send` accounts, with an FYI email
//!      to `notify_email`) or email the breakdown for a human to review + send.
//!      Anomalous cycles (0 consumed units) are always held for review, even
//!      with `auto_send` on.
//!
//! Idempotency: the `enterprise_invoices` row (UNIQUE per account+period) is the
//! gate; each external side effect is guarded by a stored id so a crash mid-flow
//! resumes rather than duplicates. Stripe idempotency keys back-stop the same.

use std::time::Duration;

use anyhow::{Context, Result};
use lit_billing_core::{StripeClient, balance, invoice};
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::time as tokio_time;

use super::types::{EnterpriseAccount, EnterpriseInvoice};
use super::{calc, db, email, period};
use crate::config::Config;
use crate::mail::Mailer;

/// Net-30 payment terms.
const DAYS_UNTIL_DUE: i64 = 30;

/// Refuse to (re)issue any money-moving Stripe call once its tracking row/attempt
/// is older than this. Stripe drops a given idempotency key after ~24h, so reusing
/// the same key beyond that posts a NEW side effect instead of deduping — a second
/// draft invoice, a doubled buffer credit, a second baseline grant. Inside the
/// window a retry is safe (Stripe dedupes); outside it we refuse and ask for human
/// verification rather than risk double-moving money. Mirrors the auto-topup
/// reconciler's 23h cap.
const MAX_RETRY_AGE_HOURS: i64 = 23;

/// Spawn the billing loop. Returns immediately.
pub fn spawn(cfg: Config, stripe: StripeClient, pool: PgPool, mailer: Mailer) {
    let interval_secs = cfg.enterprise_billing_interval_secs.max(60) as u64;
    tracing::info!("enterprise billing job: interval = {interval_secs}s");
    tokio::spawn(async move {
        let mut ticker = tokio_time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&cfg, &stripe, &pool, &mailer).await {
                tracing::warn!("enterprise billing tick failed: {e:?}");
            }
        }
    });
}

/// One sweep over all active accounts. Public for tests.
pub async fn run_once(
    cfg: &Config,
    stripe: &StripeClient,
    pool: &PgPool,
    mailer: &Mailer,
) -> Result<()> {
    let accounts = db::list_active_accounts(pool)
        .await
        .context("list active enterprise accounts")?;
    let today = OffsetDateTime::now_utc().date();
    for account in &accounts {
        if let Err(e) = run_account(cfg, stripe, pool, mailer, account, today).await {
            tracing::error!(
                account_id = account.id,
                name = %account.name,
                "enterprise billing for account failed: {e:?}"
            );
        }
    }
    Ok(())
}

async fn run_account(
    cfg: &Config,
    stripe: &StripeClient,
    pool: &PgPool,
    mailer: &Mailer,
    account: &EnterpriseAccount,
    today: time::Date,
) -> Result<()> {
    // 0. Eligibility window — checked BEFORE any money movement (baseline/regrant).
    if let Some(term_start) = account.term_start
        && today < term_start
    {
        return Ok(()); // term has not started yet
    }
    if let Some(term_end) = account.term_end
        && today >= term_end
    {
        tracing::info!(
            account_id = account.id,
            "enterprise term ended; billing paused (renewal / final invoice are manual)"
        );
        return Ok(());
    }

    // Hard invariant: any non-regrant credit on the payer account corrupts the
    // consumed = target + balance identity. Auto-topup is the only *automated*
    // credit source we can cheaply detect — refuse loudly if it's on. Manual
    // admin grants and LITKEY credits to the payer would also corrupt it; the
    // balance-range anomaly check in step 4 is the backstop for those.
    if db::payer_auto_topup_enabled(pool, &account.payer_customer_id).await? {
        anyhow::bail!(
            "payer {} has auto-topup enabled; refusing to meter (would corrupt usage accounting)",
            account.payer_customer_id
        );
    }

    // 1. Establish the buffer once (no-op after the first successful run).
    ensure_baseline(stripe, pool, account).await?;

    // 2. Anchor period to ensure an invoice for.
    let day = account.billing_anchor_day as u8;
    let anchor = period::current_anchor(today, day);
    let period_key = period::period_key(anchor);
    let prev_anchor = period::previous_anchor(anchor, day);
    let next_anchor = period::next_anchor(anchor, day);

    // 3. Missed-cycle guard. The immediately-preceding in-term anchor must
    //    already have an invoice row; otherwise a prior cycle went unbilled and
    //    folding its usage into this one (one allotment for multi-cycle usage,
    //    one committed fee for several months) would mis-bill. Refuse and surface
    //    rather than silently lump — a >1-cycle outage needs a human.
    let prev_in_term = account.term_start.is_none_or(|ts| prev_anchor >= ts);
    if prev_in_term {
        let prev_key = period::period_key(prev_anchor);
        if db::get_invoice(pool, account.id, &prev_key)
            .await?
            .is_none()
        {
            anyhow::bail!(
                "enterprise billing gap for account {}: period {prev_key} has no invoice row, so \
                 a prior cycle went unbilled. Refusing to bill {period_key} (would lump multiple \
                 cycles into one allotment / skip a committed fee). Backfill the missing period(s) \
                 manually, then this resumes automatically.",
                account.id
            );
        }
    }

    // 4. Already have a row for this period?
    if let Some(existing) = db::get_invoice(pool, account.id, &period_key).await? {
        return match existing.status.as_str() {
            // Terminal / handled — nothing to do.
            "draft" | "sent" | "paid" | "manual" => Ok(()),
            // Interrupted mid-flow — resume.
            _ => resume_invoice(cfg, stripe, pool, mailer, account, existing).await,
        };
    }

    // 5. Fresh period: snapshot consumption from the live balance and freeze it.
    let balance_cents = balance::fetch(stripe, &account.payer_customer_id)
        .await
        .context("fetch payer balance")?;
    // Metering-invariant backstop: with one regrant per cycle the balance should
    // sit between -target (full buffer) and 0. Outside that range something
    // credited or over-drew the payer outside our flow and consumed is suspect.
    if balance_cents > 0 {
        tracing::error!(
            account_id = account.id,
            balance_cents,
            "payer balance is positive (buffer exhausted); consumed reading is unreliable — \
             verify before the invoice is sent"
        );
    } else if balance_cents < -account.target_credit_cents {
        tracing::warn!(
            account_id = account.id,
            balance_cents,
            target_credit_cents = account.target_credit_cents,
            "payer credit exceeds target; an external credit likely hit the payer — overage may \
             be understated"
        );
    }
    let consumed = calc::consumed_units(account.target_credit_cents, balance_cents);
    let overage_u = calc::overage_units(consumed, account.included_units);
    let overage_c = calc::overage_cents(overage_u, account.overage_rate_hundredths_cent_per_unit);
    let total = account.committed_fee_cents + overage_c;

    let committed_period = format!("{anchor} → {next_anchor}");

    db::insert_pending_invoice(
        pool,
        account.id,
        &period_key,
        prev_anchor,
        anchor,
        &committed_period,
        consumed,
        account.included_units,
        overage_u,
        account.committed_fee_cents,
        overage_c,
        total,
    )
    .await
    .context("insert pending invoice")?;

    // Re-read (ON CONFLICT DO NOTHING means another run may already own it).
    let row = db::get_invoice(pool, account.id, &period_key)
        .await?
        .context("pending invoice row missing after insert")?;
    resume_invoice(cfg, stripe, pool, mailer, account, row).await
}

/// Drive a pending/interrupted invoice row to `draft` + review email. Each step
/// is guarded so a resumed run never duplicates a Stripe side effect.
async fn resume_invoice(
    cfg: &Config,
    stripe: &StripeClient,
    pool: &PgPool,
    mailer: &Mailer,
    account: &EnterpriseAccount,
    mut row: EnterpriseInvoice,
) -> Result<()> {
    // a. Draft invoice + line items on the INVOICE account.
    let invoice_id = match row.stripe_invoice_id.clone() {
        Some(id) => id,
        None => {
            // Durability guard (mirror of the regrant guard): past the idempotency
            // window with no recorded invoice id, a fresh create would duplicate
            // the draft (and its line items). Refuse and surface.
            let age_hours = (OffsetDateTime::now_utc() - row.created_at).whole_hours();
            if age_hours >= MAX_RETRY_AGE_HOURS {
                db::set_invoice_status(pool, row.id, "error").await?;
                anyhow::bail!(
                    "invoice {} has no recorded Stripe invoice id and is {}h old (past the {}h \
                     idempotency window); retrying could create a DUPLICATE draft. Check Stripe \
                     for an invoice with idempotency key ent_inv:{}:{} and record/clear it before \
                     retrying.",
                    row.id,
                    age_hours,
                    MAX_RETRY_AGE_HOURS,
                    account.id,
                    row.period_key
                );
            }
            let desc = format!(
                "Lit Protocol — committed {} + overage {} → {}",
                row.committed_period, row.period_start, row.period_end
            );
            let id = invoice::create_draft_invoice(
                stripe,
                &account.invoice_customer_id,
                DAYS_UNTIL_DUE,
                &desc,
                &format!("ent_inv:{}:{}", account.id, row.period_key),
            )
            .await
            .context("create draft invoice")?;

            invoice::add_invoice_item(
                stripe,
                &account.invoice_customer_id,
                &id,
                row.committed_fee_cents,
                &format!("Committed monthly fee ({})", row.committed_period),
                &format!("ent_ii_committed:{}:{}", account.id, row.period_key),
            )
            .await
            .context("add committed fee line item")?;

            if row.overage_cents > 0 {
                let d = format!(
                    "Compute overage — {} units over {} included @ $0.0025/unit ({} → {})",
                    row.overage_units, row.included_units, row.period_start, row.period_end
                );
                invoice::add_invoice_item(
                    stripe,
                    &account.invoice_customer_id,
                    &id,
                    row.overage_cents,
                    &d,
                    &format!("ent_ii_overage:{}:{}", account.id, row.period_key),
                )
                .await
                .context("add overage line item")?;
            }

            db::set_invoice_stripe_id(pool, row.id, &id).await?;
            row.stripe_invoice_id = Some(id.clone());
            id
        }
    };

    // b. Regrant the payer buffer back to target (the metering closure).
    if row.regrant_balance_txn_id.is_none() {
        if row.consumed_units <= 0 {
            // Nothing consumed this cycle (e.g. the first period right after the
            // baseline grant). Record a sentinel so we don't retry forever.
            db::set_invoice_regrant_txn(pool, row.id, "none").await?;
            row.regrant_balance_txn_id = Some("none".to_string());
        } else {
            let age_hours = (OffsetDateTime::now_utc() - row.created_at).whole_hours();
            if age_hours >= MAX_RETRY_AGE_HOURS {
                db::set_invoice_status(pool, row.id, "error").await?;
                anyhow::bail!(
                    "invoice {} regrant is unconfirmed and {}h old (past the {}h idempotency \
                     window). The credit MAY have already landed — do NOT blindly re-credit. \
                     Check the payer's Stripe balance_transactions for idempotency key \
                     ent_regrant:{}:{}: if it exists, record the txn id on this row; only credit \
                     manually if it is truly absent.",
                    row.id,
                    age_hours,
                    MAX_RETRY_AGE_HOURS,
                    account.id,
                    row.period_key
                );
            }
            let regrant_key = format!("ent_regrant:{}:{}", account.id, row.period_key);
            let txn = balance::write_transaction(
                stripe,
                &account.payer_customer_id,
                -row.consumed_units, // negative = credit; restores buffer to target
                &format!("Enterprise buffer regrant — {}", row.period_key),
                Some(&regrant_key),
            )
            .await
            .context("regrant payer buffer")?;
            db::set_invoice_regrant_txn(pool, row.id, &txn).await?;
            row.regrant_balance_txn_id = Some(txn);
        }
    }

    // c. Deliver. `auto_send` accounts get finalize + send with an FYI email;
    //    otherwise the draft is left for a human to review + send. A 0-unit
    //    cycle means the consumed reading is suspect (external credit hit the
    //    payer, or usage genuinely stopped) — hold those for review even with
    //    auto_send on, so a mis-metered invoice never goes out unseen.
    let invoice_url = format!(
        "{}/invoices/{}",
        cfg.stripe_dashboard_base.trim_end_matches('/'),
        invoice_id
    );
    if account.auto_send && row.consumed_units > 0 {
        // Durability guard (same class as the create/regrant guards). Past the
        // idempotency window a finalize replay can't double-charge (Stripe
        // rejects re-finalizing), but the customer MAY already have been
        // emailed — hand off to a human instead of guessing.
        let age_hours = (OffsetDateTime::now_utc() - row.created_at).whole_hours();
        if age_hours >= MAX_RETRY_AGE_HOURS {
            db::set_invoice_status(pool, row.id, "error").await?;
            anyhow::bail!(
                "invoice {} finalize+send is unconfirmed and {}h old (past the {}h idempotency \
                 window). The invoice MAY already be finalized and emailed to the customer. \
                 Check invoice {} in Stripe: if it is open/paid, mark this row 'sent'; if it is \
                 still a draft, send it from the dashboard and mark this row 'manual'.",
                row.id,
                age_hours,
                MAX_RETRY_AGE_HOURS,
                invoice_id
            );
        }
        invoice::finalize_and_send(
            stripe,
            &invoice_id,
            &format!("ent_send:{}:{}", account.id, row.period_key),
        )
        .await
        .context("finalize and send invoice")?;
        email::send_sent_email(mailer, account, &row, &invoice_url)
            .await
            .context("send sent-notification email")?;
        db::set_invoice_sent(pool, row.id, OffsetDateTime::now_utc()).await?;

        tracing::info!(
            account_id = account.id,
            period = %row.period_key,
            invoice = %invoice_id,
            total_cents = row.total_cents,
            overage_units = row.overage_units,
            "enterprise invoice finalized and sent; notification email sent"
        );
        return Ok(());
    }

    email::send_review_email(mailer, account, &row, &invoice_url)
        .await
        .context("send review email")?;
    db::set_invoice_drafted(pool, row.id, OffsetDateTime::now_utc()).await?;

    tracing::info!(
        account_id = account.id,
        period = %row.period_key,
        invoice = %invoice_id,
        total_cents = row.total_cents,
        overage_units = row.overage_units,
        "enterprise invoice drafted; review email sent"
    );
    Ok(())
}

/// One-time: bring the payer's credit up to `target_credit_cents`. Idempotent —
/// gated on `baseline_granted_at`; never claws credit back if already funded.
async fn ensure_baseline(
    stripe: &StripeClient,
    pool: &PgPool,
    account: &EnterpriseAccount,
) -> Result<()> {
    if account.baseline_granted_at.is_some() {
        return Ok(());
    }
    // Retry-window guard (same class as the regrant/invoice guards). If a prior
    // attempt may have credited the baseline but we never recorded success, the
    // idempotency key (ent_baseline:{id}) only dedupes for ~24h. Past that we
    // cannot safely re-credit a full $target buffer.
    if let Some(attempted) = account.baseline_attempted_at {
        let age_hours = (OffsetDateTime::now_utc() - attempted).whole_hours();
        if age_hours >= MAX_RETRY_AGE_HOURS {
            anyhow::bail!(
                "baseline grant for account {} is unconfirmed and {}h since first attempt (past \
                 the {}h idempotency window). The credit MAY have already landed — do NOT blindly \
                 re-credit. Check the payer's Stripe balance_transactions for idempotency key \
                 ent_baseline:{}: if it exists, set baseline_granted_at on the account; only \
                 credit manually if it is truly absent.",
                account.id,
                age_hours,
                MAX_RETRY_AGE_HOURS,
                account.id
            );
        }
    } else {
        // Stamp the window-start BEFORE touching Stripe so a crash between the
        // write and recording success is bounded by the guard above.
        db::mark_baseline_attempted(pool, account.id).await?;
    }
    let balance_cents = balance::fetch(stripe, &account.payer_customer_id)
        .await
        .context("fetch payer balance for baseline")?;
    let amount = calc::regrant_amount_cents(account.target_credit_cents, balance_cents);
    if amount >= 0 {
        // Already at/above target — record baseline without crediting.
        db::mark_baseline(pool, account.id, "none").await?;
        tracing::info!(
            account_id = account.id,
            "enterprise baseline: payer already funded ≥ target; no credit written"
        );
        return Ok(());
    }
    let baseline_key = format!("ent_baseline:{}", account.id);
    let txn = balance::write_transaction(
        stripe,
        &account.payer_customer_id,
        amount,
        &format!(
            "Enterprise credit buffer (baseline → ${})",
            account.target_credit_cents / 100
        ),
        Some(&baseline_key),
    )
    .await
    .context("write baseline buffer credit")?;
    db::mark_baseline(pool, account.id, &txn).await?;
    tracing::info!(
        account_id = account.id,
        amount_cents = amount,
        "enterprise baseline buffer established"
    );
    Ok(())
}
