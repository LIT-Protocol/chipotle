//! LITKEY partial-credit reconciler (CPL-375).
//!
//! `chain::handle_confirmed_litkey_payment` inserts the `litkey_payments`
//! row BEFORE it writes the Stripe `balance_transaction`, then fills in
//! `stripe_balance_transaction_id` once the credit lands. A crash or Stripe
//! error between those two steps leaves a "partial" credited row: the DB
//! knows the credit is owed but the balance_transactions write never
//! completed.
//!
//! This reconciler sweeps those partials every `RECONCILER_INTERVAL_SECS`
//! and replays the Stripe credit using the SAME
//! `litkey:{chain}:{tx}:{log_index}` idempotency key the live path used.
//! Because the key is stable:
//!   - if the original credit DID land (crash after Stripe, before UPDATE),
//!     Stripe dedupes and returns the existing balance_transaction;
//!   - if it did NOT (crash/error before Stripe), Stripe credits fresh.
//!
//! Either way the customer is credited exactly once.
//!
//! Two age gates bound the work, mirroring the auto_topup reconciler:
//!   - `MIN_PARTIAL_AGE_SECS`: a fresh partial is almost certainly the live
//!     claim still in-flight; skip it to avoid a pointless (though
//!     idempotency-safe) race on the balance_transactions write.
//!   - `MAX_PARTIAL_RETRY_AGE_HOURS`: past Stripe's ~24h Idempotency-Key TTL
//!     the key no longer dedupes, so replaying it would risk the very
//!     double-credit this whole change exists to prevent. Older rows are
//!     logged loudly for manual repair and skipped.

use std::time::Duration;

use ::time::OffsetDateTime;
use lit_billing_core::StripeClient;
use sqlx::PgPool;
use tokio::time as tokio_time;

use crate::chain::{self, PartialLitkeyCredit};
use crate::config::Config;

/// A partial younger than this is overwhelmingly likely to be the live claim
/// path's own in-flight balance_transactions write, not a real orphan.
/// Retrying it would race the claim; the shared idempotency key keeps that
/// race safe but it is wasteful and noisy. Mirrors the auto_topup floor.
///
/// The claim path completes fresh partials synchronously (no min-age gate),
/// so this only defers the *background* sweep; a true crash mid-flow still
/// recovers within one tick if the user never re-claims.
const MIN_PARTIAL_AGE_SECS: i64 = 60;

/// Spawn the reconciler. Returns immediately; the loop runs in the
/// background for the lifetime of the process. The first tick fires
/// immediately so a process restart repairs any partial left by the crash
/// that triggered the restart.
pub fn spawn(config: Config, stripe: StripeClient, pool: PgPool) {
    let interval_secs = config.reconciler_interval_secs.max(10) as u64;
    tracing::info!("litkey partial-credit reconciler: interval = {interval_secs}s");
    tokio::spawn(async move {
        let mut ticker = tokio_time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&stripe, &pool).await {
                tracing::warn!("litkey reconciler tick failed: {e:?}");
            }
        }
    });
}

/// One sweep over the partial credited rows. Public for tests; the
/// production scheduler in [`spawn`] is the only other caller.
pub async fn run_once(stripe: &StripeClient, pool: &PgPool) -> anyhow::Result<()> {
    let partials = chain::list_partial_litkey_credits(pool).await?;
    if partials.is_empty() {
        return Ok(());
    }
    let now = OffsetDateTime::now_utc();
    for partial in partials {
        if let Err(e) = repair_partial(stripe, pool, &partial, now).await {
            tracing::warn!(
                tx_hash = %format!("{:#x}", partial.log.tx_hash),
                log_index = partial.log.log_index,
                "litkey reconciler: repair failed, will retry next tick: {e:?}"
            );
        }
    }
    Ok(())
}

async fn repair_partial(
    stripe: &StripeClient,
    pool: &PgPool,
    partial: &PartialLitkeyCredit,
    now: OffsetDateTime,
) -> anyhow::Result<()> {
    let tx_hash = format!("{:#x}", partial.log.tx_hash);
    let log_index = partial.log.log_index;
    let age = now - partial.credited_at;

    // Too fresh: the live claim is probably still finishing its own credit.
    if age.whole_seconds() < MIN_PARTIAL_AGE_SECS {
        return Ok(());
    }

    // Too old: the idempotency key can no longer dedupe. Replaying would risk
    // a double-credit — exactly what this change prevents. Flag for a human.
    if partial.past_idempotency_window(now) {
        tracing::error!(
            tx_hash = %tx_hash,
            log_index,
            customer_id = %partial.stripe_customer_id,
            age_hours = age.whole_hours(),
            "litkey reconciler: partial credit older than Stripe idempotency window; \
             manual repair needed (skipping to avoid double-credit risk)"
        );
        return Ok(());
    }

    tracing::warn!(
        tx_hash = %tx_hash,
        log_index,
        customer_id = %partial.stripe_customer_id,
        "litkey reconciler: completing partial credit"
    );
    chain::complete_partial_litkey_credit(stripe, pool, partial).await?;
    Ok(())
}
