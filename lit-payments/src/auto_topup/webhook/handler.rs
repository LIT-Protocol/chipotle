//! `POST /stripe/webhook` — `customer.updated` handler.
//!
//! Implements the full §6 flow synchronously inside the webhook request:
//! verify signature → quick exits → mutex → fresh balance → list PIs →
//! failure/cap check → off-session PaymentIntent create → sync credit →
//! cache invalidation. Returns 5xx on transient backend failures so
//! Stripe retries (3-day window); 200 only after credit work commits.
//!
//! The handler is generously instrumented because Phase 5 is the
//! load-bearing trigger — when it misbehaves in production, observability
//! is the difference between a 1-hour incident and a 1-day one.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use lit_billing_core::StripeClient;
use rocket::data::{Data, ToByteUnit};
use rocket::http::{Header, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::{State, post};
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auto_topup::db;
use crate::auto_topup::webhook::mutex::PerCustomerMutex;
use crate::auto_topup::webhook::sca::generate_recovery_token;
use crate::auto_topup::webhook::signature::verify as verify_signature;
use crate::config::Config;
use crate::internal::client as internal_client;
use crate::mail::Mailer;

const MAX_BODY_BYTES: u64 = 1024 * 1024;

/// Minimum top-up floor — same value the dashboard validation enforces in
/// `billing::auto_topup_config`. Kept here as a documented constant for the
/// failure-threshold test and for future use by spec-derived assertions.
#[allow(dead_code)]
const MIN_TOPUP_CENTS: i64 = 500;

const FAILURE_DISABLE_THRESHOLD: usize = 3;

/// Webhook-specific request guard that extracts the Stripe-Signature
/// header. Returning here as `Option<String>` lets the handler decide
/// whether absence is a hard 401 or some other shape.
pub struct StripeSignatureHeader(pub Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for StripeSignatureHeader {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let value = req
            .headers()
            .get_one("Stripe-Signature")
            .map(|s| s.to_string());
        Outcome::Success(StripeSignatureHeader(value))
    }
}

#[post("/stripe/webhook", data = "<body>")]
pub async fn stripe_webhook(
    sig: StripeSignatureHeader,
    body: Data<'_>,
    cfg: &State<Config>,
    stripe: &State<StripeClient>,
    pool: &State<PgPool>,
    mutex_cache: &State<PerCustomerMutex>,
    mailer: &State<Mailer>,
) -> Status {
    let raw = match body.open(MAX_BODY_BYTES.bytes()).into_bytes().await {
        Ok(v) if v.is_complete() => v.into_inner(),
        Ok(_) => {
            tracing::warn!("stripe webhook: body exceeded {MAX_BODY_BYTES} bytes");
            return Status::PayloadTooLarge;
        }
        Err(e) => {
            tracing::warn!("stripe webhook: read body failed: {e}");
            return Status::BadRequest;
        }
    };

    let Some(header_value) = sig.0 else {
        return Status::Unauthorized;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if let Err(e) = verify_signature(&header_value, &raw, &cfg.stripe_webhook_secret, now) {
        tracing::warn!("stripe webhook: signature rejected: {e}");
        return Status::Unauthorized;
    }

    let event: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("stripe webhook: body not JSON: {e}");
            return Status::BadRequest;
        }
    };

    // Step 2: Filter event type AND require a balance change. The latter
    // is the cheap "not for us" reject for customer.updated events that
    // change email / metadata / etc. without touching balance.
    if event.get("type").and_then(|v| v.as_str()) != Some("customer.updated") {
        return Status::Ok;
    }
    if event.pointer("/data/previous_attributes/balance").is_none() {
        return Status::Ok;
    }

    let obj = match event.pointer("/data/object") {
        Some(o) => o,
        None => return Status::Ok,
    };
    let customer_id = match obj.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return Status::Ok,
    };
    let new_balance = obj.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
    // Codex P1 (Phase 5): use the Stripe webhook event.id as the
    // idempotency-key seed for `paymentIntents.create`. Stripe redelivers
    // the same event with the same id on 5xx replies, so a stable key
    // means Stripe dedupes the retry on its side instead of letting us
    // create a second PI per retry.
    let event_id = event
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match process_event(
        cfg.inner(),
        stripe.inner(),
        pool.inner(),
        mutex_cache.inner(),
        mailer.inner(),
        &customer_id,
        new_balance,
        &event_id,
    )
    .await
    {
        Ok(()) => Status::Ok,
        Err(ProcessError::Transient(e)) => {
            tracing::error!("stripe webhook: transient error: {e:?}");
            Status::ServiceUnavailable
        }
    }
}

#[derive(Debug)]
enum ProcessError {
    Transient(anyhow::Error),
}

impl From<anyhow::Error> for ProcessError {
    fn from(e: anyhow::Error) -> Self {
        ProcessError::Transient(e)
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_event(
    cfg: &Config,
    stripe: &StripeClient,
    pool: &PgPool,
    mutex_cache: &PerCustomerMutex,
    mailer: &Mailer,
    customer_id: &str,
    payload_balance: i64,
    event_id: &str,
) -> Result<(), ProcessError> {
    // Step 3a (pre-mutex): cheap payload-only quick exit. We re-read the
    // config under the mutex below; this lookup is just to avoid taking
    // the mutex when the row is plainly disabled or the payload balance
    // already exceeds the threshold. Codex P1 (Phase 5): the
    // load-bearing config read must happen INSIDE the mutex so that a
    // concurrent SCA-staging path that sets pending_action_pi_id is
    // observed by the next webhook tick. Without that ordering, two
    // overlapping customer.updated deliveries can both see
    // pending_action_pi_id=None and each fire an off-session PI.
    let pre_check = db::get_by_customer_id(pool, customer_id)
        .await
        .context("db read auto_topup_config (pre-mutex)")?;
    // Stale pending recovery — only clear when the PI is definitively
    // dead at Stripe OR the recovery token has expired. We deliberately
    // do NOT clear on `requires_payment_method + last_payment_error =
    // authentication_required`: that's the LIVE state the recovery
    // email + `/recover_topup.html` flow is built to handle. Clearing
    // it on every subsequent `customer.updated` would invalidate the
    // in-flight email and re-send a duplicate one each time the user
    // ran a Lit Action.
    //
    // The two cases this catches:
    //   1. Token expired (>24h since SCA was staged) — user never
    //      completed 3DS.
    //   2. PI is `canceled` / `succeeded` at Stripe — terminal state,
    //      our DB just got behind.
    //
    // Anything else (including the off-session `authentication_required`
    // failure state) is treated as live and left alone. The 24h token
    // expiry is the upper bound on how long pending state can persist.
    if let Some(c) = &pre_check
        && c.enabled
        && let Some(pi_id) = c.pending_action_pi_id.as_deref()
    {
        let token_expired = c
            .recovery_token_expires_at
            .map(|exp| exp < OffsetDateTime::now_utc())
            .unwrap_or(false);
        let pi_terminal = match stripe.get(&format!("payment_intents/{pi_id}"), &[]).await {
            Ok(resp) => {
                let status = resp
                    .body
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // Only "definitely done at Stripe" qualifies as terminal.
                // `requires_action` is the happy-path pending state and
                // `requires_payment_method` (when paired with
                // last_payment_error.code = authentication_required) is
                // the in-flight SCA recovery state we want to preserve.
                matches!(status, "canceled" | "succeeded")
            }
            Err(e) => {
                // If we can't reach Stripe, be conservative and don't
                // clear; the next tick will retry. Don't escalate to
                // a transient error — the webhook should still 200 so
                // Stripe doesn't pile retries.
                tracing::warn!(
                    customer_id,
                    pi_id,
                    "could not fetch PI status to evaluate stale pending; leaving as-is: {e}"
                );
                false
            }
        };
        if token_expired || pi_terminal {
            tracing::warn!(
                customer_id,
                pi_id,
                token_expired,
                pi_terminal,
                "pending SCA recovery stale; clearing pending state and trying fresh"
            );
            let _ = db::clear_pending_action_force(pool, customer_id).await;
        }
    }
    // Re-read after possible clear above.
    let pre_check = db::get_by_customer_id(pool, customer_id)
        .await
        .context("db read auto_topup_config (pre-mutex re-read)")?;
    let pre_threshold = match &pre_check {
        Some(c) if c.enabled && c.pending_action_pi_id.is_none() => c.threshold_cents,
        _ => return Ok(()),
    };
    if let Some(t) = pre_threshold
        && -payload_balance >= t
    {
        return Ok(());
    }

    // Step 4: Acquire per-customer mutex. Serializes concurrent
    // customer.updated deliveries for the same customer.
    let mutex = mutex_cache.get(customer_id);
    let _guard = mutex.lock().await;

    // Step 4b: Re-load config under the mutex. Required for correctness
    // when another delivery (or the SCA-staging path) raced ahead of us
    // and changed enabled / pending_action_pi_id between our pre-check
    // and the lock acquisition.
    let config = match db::get_by_customer_id(pool, customer_id)
        .await
        .context("db read auto_topup_config (post-mutex)")?
    {
        Some(c) if c.enabled && c.pending_action_pi_id.is_none() => c,
        _ => return Ok(()),
    };
    let threshold = config.threshold_cents.ok_or_else(|| {
        ProcessError::Transient(anyhow::anyhow!(
            "enabled config missing threshold for {customer_id} — CHECK constraint should have caught this"
        ))
    })?;

    // Step 5: Fresh balance fetch — don't trust the webhook payload after
    // the mutex wait; the balance may have moved.
    let fresh_balance = lit_billing_core::balance::fetch(stripe, customer_id)
        .await
        .context("fetch fresh balance")?;
    if -fresh_balance >= threshold {
        return Ok(());
    }

    // Step 6: List PIs this month. Stripe doesn't filter by metadata
    // server-side on list, so we filter client-side.
    let month_start = month_start_unix(OffsetDateTime::now_utc());
    let pis = list_pis_since(stripe, customer_id, month_start)
        .await
        .context("list PIs")?;

    // Step 7: Derive consecutive failure count from most recent PIs.
    // Disabling at >=3 matches plan §6 + the spec the team agreed on.
    let consecutive_failures = count_consecutive_failures(&pis);
    if consecutive_failures >= FAILURE_DISABLE_THRESHOLD {
        db::disable_after_failures(pool, customer_id)
            .await
            .context("disable after failures")?;
        // TODO Phase 8: dispatch "card needs updating" email via Resend.
        tracing::warn!(
            customer_id,
            consecutive_failures,
            "auto top-up disabled after consecutive failures"
        );
        return Ok(());
    }

    // Step 8: Cap check. Sum amounts of all non-failed PIs this month.
    // CPL-379 L9: the DB CHECK constraint makes this non-null whenever
    // auto-top-up is enabled, but the crate forbids `.expect()` on the hot
    // path — a NULL here would be an invariant violation that panics the
    // webhook worker. Skip the charge and alert instead; a retry won't heal a
    // constraint violation, so return Ok(()) rather than a transient error.
    let Some(topup_amount) = config.topup_amount_cents else {
        tracing::error!(
            customer_id,
            "auto top-up enabled but topup_amount_cents is NULL (DB CHECK invariant violated); skipping charge"
        );
        return Ok(());
    };
    // monthly_cap_cents is optional — None = unlimited (only the
    // per-charge MAX_TOPUP_CENTS cap applies). When set, enforce the
    // soft cap as before.
    let monthly_cap = config.monthly_cap_cents;
    let spend_so_far = month_spend_cents(&pis);
    if let Some(cap) = monthly_cap
        && spend_so_far + topup_amount > cap
    {
        tracing::info!(
            customer_id,
            spend_so_far,
            topup_amount,
            monthly_cap = cap,
            "auto top-up cap reached; skipping charge"
        );
        return Ok(());
    }

    // Step 9: Create off-session PaymentIntent. Codex P1 (Phase 5): the
    // idempotency-key MUST derive from the Stripe event.id so that when
    // Stripe redelivers the same customer.updated event (which it does
    // on every 5xx we return), `paymentIntents.create` gets deduped on
    // Stripe's side instead of creating a second PI per retry. The 24h
    // idempotency-key TTL is the right scope — well past Stripe's 3-day
    // retry window for an individual event would risk false hits, but
    // within a redelivery burst it's exactly what we want. Falls back
    // to a UUID if event_id is missing (only the unit-test paths).
    // CPL-379 L9: same invariant as topup_amount above — non-null by DB CHECK
    // when enabled, but never `.expect()` on the hot path.
    let Some(payment_method_id) = config.payment_method_id.as_deref() else {
        tracing::error!(
            customer_id,
            "auto top-up enabled but payment_method_id is NULL (DB CHECK invariant violated); skipping charge"
        );
        return Ok(());
    };
    let wallet_address = config.wallet_address.as_str();
    let amount_str = topup_amount.to_string();
    let idempotency_key = if event_id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        format!("auto_topup_pi:{event_id}")
    };
    let pi_resp = stripe
        .post_with_idempotency(
            "payment_intents",
            &[
                ("amount", amount_str.as_str()),
                ("currency", "usd"),
                ("customer", customer_id),
                ("payment_method", payment_method_id),
                ("off_session", "true"),
                ("confirm", "true"),
                ("metadata[source]", "auto_topup"),
                ("metadata[wallet_address]", wallet_address),
            ],
            &idempotency_key,
        )
        .await;

    let pi_body = match pi_resp {
        Ok(r) => r.body,
        Err(e) => {
            // Distinguish "Stripe returned a structured error with
            // error.code" (decline / sca / etc.) from "HTTP timed out
            // before any response" (transient — reconciler handles).
            //
            // Codex P1 #1: read `error.payment_intent.id` from the
            // structured StripeError body instead of regexing the Display
            // string. Without this, SCA recovery stages whatever string
            // we happened to format as `pending_action_pi_id`, and the
            // recovery page later calls Stripe with a non-PI id.
            if let Some(stripe_err) = e.downcast_ref::<lit_billing_core::StripeError>() {
                let code = stripe_err.code().unwrap_or("");
                // Codex P1 (Phase 5): structured Stripe 5xx (HTTP 5xx
                // with a JSON `error` body, typically error.type=api_error)
                // is a transient failure — Stripe's side is degraded, not
                // a permanent decline. Pre-fix this collapsed to Ok(()),
                // which acked the webhook and lost the retry. Returning
                // Transient surfaces the 503 to Stripe; their 3-day
                // redelivery loop is exactly the right retry primitive.
                if stripe_err.status.is_server_error() {
                    return Err(ProcessError::Transient(anyhow::anyhow!(
                        "Stripe {} on paymentIntents.create: {stripe_err}",
                        stripe_err.status
                    )));
                }
                if code == "authentication_required" {
                    let pi_id = stripe_err
                        .payment_intent_id()
                        .ok_or_else(|| {
                            ProcessError::Transient(anyhow::anyhow!(
                                "Stripe returned authentication_required without error.payment_intent.id; \
                                 cannot stage SCA recovery"
                            ))
                        })?;
                    handle_sca_required(cfg, stripe, mailer, pool, customer_id, pi_id).await?;
                    return Ok(());
                }
                if matches!(
                    code,
                    "card_declined"
                        | "expired_card"
                        | "insufficient_funds"
                        | "incorrect_cvc"
                        | "processing_error"
                ) {
                    // TODO Phase 8: dispatch decline email. Failure
                    // counter derivation in step 7 handles auto-disable.
                    tracing::warn!(customer_id, code, "auto top-up charge declined");
                    return Ok(());
                }
                // Other Stripe-side errors: not retriable here, but log
                // and let the reconciler / next webhook tick figure it
                // out. Return Ok(200) so Stripe doesn't pile on retries.
                tracing::warn!(
                    customer_id,
                    code,
                    "auto top-up: unexpected Stripe error code"
                );
                return Ok(());
            }
            // Non-Stripe error (network timeout, JSON parse, 5xx with no
            // structured body). Treat as transient — Stripe retries the
            // webhook; the reconciler catches any orphaned succeeded PI.
            return Err(ProcessError::Transient(anyhow::anyhow!(
                "paymentIntents.create transient error: {e:?}"
            )));
        }
    };

    let pi_id = pi_body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe PI response missing id"))?
        .to_string();
    let pi_status = pi_body.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if pi_status != "succeeded" {
        // Stripe occasionally returns `requires_action` with status code 200
        // (not as an error). Treat it the same way.
        if pi_status == "requires_action" {
            handle_sca_required(cfg, stripe, mailer, pool, customer_id, &pi_id).await?;
            return Ok(());
        }
        tracing::warn!(
            customer_id,
            pi_id,
            pi_status,
            "PI not succeeded; skip credit"
        );
        return Ok(());
    }

    // Step 10: Try to claim the credit slot. ON CONFLICT DO NOTHING means
    // a concurrent webhook + reconciler attempting the same PI converge
    // on exactly one credit row.
    let claimed = db::try_insert_credit(pool, &pi_id, customer_id, topup_amount)
        .await
        .context("claim credit row")?;
    if !claimed {
        // Someone else (other replica, reconciler) is/has credited this PI.
        // Safe to return — the credit lands either way.
        return Ok(());
    }

    // Step 11: Write the balance transaction. Idempotency-Key locks the
    // logical "credit for PI X" operation at Stripe's side too.
    let credit_idem = format!("credit:{pi_id}");
    let neg_amount = (-topup_amount).to_string();
    let bt_resp = stripe
        .post_with_idempotency(
            &format!("customers/{customer_id}/balance_transactions"),
            &[
                ("amount", neg_amount.as_str()),
                ("currency", "usd"),
                ("description", &format!("Auto top-up via {pi_id}")),
            ],
            &credit_idem,
        )
        .await
        .context("balance_transactions write")?;
    let bt_id = bt_resp
        .body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Stripe balance_tx response missing id"))?
        .to_string();

    // Step 12: Mark the credit complete in our ledger.
    db::mark_credit_completed(pool, &pi_id, &bt_id)
        .await
        .context("mark credit completed")?;

    // Step 12b: If this PI was the SCA recovery target, clear pending state.
    if config.pending_action_pi_id.as_deref() == Some(pi_id.as_str()) {
        let _ = db::clear_pending_action(pool, customer_id, &pi_id).await;
    }

    // Step 13: Invalidate lit-api-server's balance cache. Fire-and-forget;
    // a failure here means the api-server serves the stale balance for up
    // to 10 minutes (TTL). Not a correctness problem; just freshness.
    let _ = invalidate_cache(cfg, customer_id).await;

    Ok(())
}

fn month_start_unix(now: OffsetDateTime) -> i64 {
    let first = OffsetDateTime::new_utc(
        time::Date::from_calendar_date(now.year(), now.month(), 1)
            .expect("first-of-month is always valid"),
        time::Time::MIDNIGHT,
    );
    first.unix_timestamp()
}

async fn list_pis_since(
    stripe: &StripeClient,
    customer_id: &str,
    since_unix: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut starting_after: Option<String> = None;
    let since_str = since_unix.to_string();
    loop {
        let mut params: Vec<(&str, &str)> = vec![
            ("customer", customer_id),
            ("limit", "100"),
            ("created[gte]", since_str.as_str()),
        ];
        if let Some(ref s) = starting_after {
            params.push(("starting_after", s.as_str()));
        }
        let resp = stripe
            .get("payment_intents", &params)
            .await
            .context("payment_intents.list")?;
        let data = resp.body.get("data").and_then(|d| d.as_array()).cloned();
        let Some(arr) = data else { break };
        let has_more = resp
            .body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut last_id: Option<String> = None;
        for pi in arr.iter() {
            if pi.pointer("/metadata/source").and_then(|v| v.as_str()) == Some("auto_topup") {
                out.push(pi.clone());
            }
            last_id = pi.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if !has_more {
            break;
        }
        match last_id {
            Some(id) => starting_after = Some(id),
            None => break,
        }
    }
    Ok(out)
}

fn count_consecutive_failures(pis: &[Value]) -> usize {
    // The PI list endpoint returns newest first by default; we iterate in
    // that order and stop counting at the first non-failed PI.
    let mut count = 0;
    for pi in pis {
        if is_failed(pi) {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn is_failed(pi: &Value) -> bool {
    let status = pi.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status == "requires_payment_method" {
        return true;
    }
    let last_err_code = pi
        .pointer("/last_payment_error/code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    matches!(
        last_err_code,
        "card_declined"
            | "expired_card"
            | "insufficient_funds"
            | "incorrect_cvc"
            | "processing_error"
    )
}

/// Sum `amount` over all non-failed, non-abandoned PIs in the window.
///
/// Codex P1 (Phase 5): `requires_action` is neither failed nor succeeded —
/// it's an SCA challenge the user never completed. Pre-fix this code
/// counted those PI amounts toward the monthly cap, so a single 3DS
/// drop-off could exhaust the user's budget without any successful
/// charge. The cap is "successful spend this month plus what's still
/// in-flight"; we explicitly exclude `requires_action` (the user has
/// abandoned the attempt or hasn't returned to complete it). The
/// `pending_action_pi_id` state on the config row already paused new
/// charges while a 3DS challenge is in flight, so we don't double-block.
fn month_spend_cents(pis: &[Value]) -> i64 {
    pis.iter()
        .filter(|pi| !is_failed(pi))
        .filter(|pi| pi.get("status").and_then(|v| v.as_str()) != Some("requires_action"))
        .filter_map(|pi| pi.get("amount").and_then(|v| v.as_i64()))
        .sum()
}

async fn handle_sca_required(
    cfg: &Config,
    stripe: &StripeClient,
    mailer: &Mailer,
    pool: &PgPool,
    customer_id: &str,
    pi_id: &str,
) -> Result<(), ProcessError> {
    let token = generate_recovery_token();
    db::set_pending_action(pool, customer_id, pi_id, &token)
        .await
        .context("set pending_action")?;

    // Fetch the PI to pull `amount` + `payment_method` for a richer
    // email body — users should see exactly how much, on which card,
    // before authorising the 3DS challenge.
    let (charge_amount_dollars, card_brand, card_last4) =
        fetch_pi_details(stripe, pi_id).await.unwrap_or_default();

    // Glitch's PR review #2: actually send the recovery email. Without
    // this the user has no way to discover the recovery URL — the token
    // is staged in the DB but the link never reaches the cardholder, so
    // the 3DS challenge sits forever and auto top-up is permanently
    // paused. Fire-and-forget — a Resend outage shouldn't fail the
    // webhook (Stripe would retry and stage a second pending PI), but
    // we log loudly so the on-call sees email delivery degradation.
    // Static dashboard ships `recover_topup.html` at the path root —
    // we use the explicit `.html` suffix in the link so it works
    // regardless of whether the host strips extensions. Pre-fix the
    // URL was `/recover_topup` and depended on Cloudflare Pages'
    // default extension-stripping; any host without that behaviour
    // (custom CDN, future S3 mount, local Vite dev) 404'd the link.
    let recovery_url = format!(
        "{}/recover_topup.html?token={}",
        cfg.public_base_url.trim_end_matches('/'),
        token
    );
    let email_result: Result<(), anyhow::Error> = match fetch_customer_email(stripe, customer_id)
        .await
    {
        Ok(Some(email)) => {
            let amount_line = if !charge_amount_dollars.is_empty() {
                format!("Amount: ${charge_amount_dollars}\n")
            } else {
                String::new()
            };
            let card_line = if !card_brand.is_empty() && !card_last4.is_empty() {
                format!("Card: {card_brand} •••• {card_last4}\n")
            } else {
                String::new()
            };
            let subject = if !charge_amount_dollars.is_empty() {
                format!(
                    "Action required: confirm your ${charge_amount_dollars} Lit Protocol auto top-up"
                )
            } else {
                "Action required: complete your Lit Protocol auto top-up".to_string()
            };
            let text = format!(
                "Lit Protocol tried to auto top-up your account, but your card requires \
                     extra verification (3D Secure) before the charge can complete.\n\n\
                     {amount_line}{card_line}\
                     Confirm the charge by opening the link below — it expires in 24 hours and \
                     can only be used once.\n\n{recovery_url}\n\n\
                     If you didn't expect this, you can safely ignore the email; auto top-up \
                     stays paused until you click the link or update your card."
            );
            let amount_html = if !charge_amount_dollars.is_empty() {
                format!(
                    "<p style=\"margin:0;\"><strong>Amount:</strong> ${charge_amount_dollars}</p>"
                )
            } else {
                String::new()
            };
            let card_html = if !card_brand.is_empty() && !card_last4.is_empty() {
                format!(
                    "<p style=\"margin:0;\"><strong>Card:</strong> {card_brand} •••• {card_last4}</p>"
                )
            } else {
                String::new()
            };
            let html = format!(
                "<p><strong>Lit Protocol</strong> tried to auto top-up your account, but \
                     your card requires extra verification (3D Secure) before the charge can \
                     complete.</p>\
                     <div style=\"background:#f5f5f7;padding:12px 16px;border-radius:6px;margin:16px 0;\">\
                     {amount_html}{card_html}\
                     </div>\
                     <p><a href=\"{recovery_url}\" style=\"display:inline-block;padding:10px 18px;background:#4338ca;color:white;text-decoration:none;border-radius:6px;\">Confirm the charge</a></p>\
                     <p style=\"color:#666;font-size:0.9em\">This link expires in 24 hours and \
                     can only be used once. If you didn't expect this, you can safely ignore \
                     this email; auto top-up stays paused until you click the link or update \
                     your card.</p>"
            );
            match mailer.send(&email, &subject, &html, &text).await {
                Ok(()) => {
                    tracing::info!(customer_id, pi_id, "SCA recovery email dispatched");
                    Ok(())
                }
                Err(e) => {
                    tracing::error!(customer_id, pi_id, "SCA recovery email send failed: {e}");
                    Err(anyhow::anyhow!("SCA recovery email send failed: {e}"))
                }
            }
        }
        Ok(None) => {
            // No email on file — nothing we can do. Don't roll back the
            // pending state; the user has no recovery channel and the
            // dashboard "action required" banner is the only signal left.
            // Reconciler / next webhook will not re-fire (pending_action_pi_id
            // is set), so the SCA stays paused indefinitely until the user
            // updates their account email or contacts support.
            tracing::warn!(
                customer_id,
                pi_id,
                "Stripe customer has no email on file; SCA recovery link cannot be delivered"
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                customer_id,
                pi_id,
                "fetch customer email for SCA recovery failed: {e}"
            );
            Err(anyhow::anyhow!(
                "fetch customer email for SCA recovery failed: {e}"
            ))
        }
    };

    // Codex P1 (Phase 5): if the email (or email-lookup) failed, the
    // pending state we just staged is useless — the user has no link to
    // click and subsequent customer.updated webhooks short-circuit on
    // `pending_action_pi_id`. Roll the pending state back so the next
    // webhook tick re-attempts SCA staging from scratch (which will
    // mint a fresh recovery token). Single-use semantics still hold;
    // tokens are by-pi-id so a stale token in a delayed email is
    // automatically invalidated once we restage. We DO NOT return
    // Transient — Stripe shouldn't keep retrying the same delivery
    // forever; the user's next manual top-up or balance change will
    // produce a fresh customer.updated event.
    if let Err(e) = email_result {
        if let Err(clear_err) = db::clear_pending_action(pool, customer_id, pi_id).await {
            tracing::error!(
                customer_id,
                pi_id,
                "SCA recovery rollback failed (pending state stuck): {clear_err}; orig: {e}"
            );
        } else {
            tracing::warn!(
                customer_id,
                pi_id,
                "SCA recovery email failed; pending state rolled back so next webhook retries"
            );
        }
        return Ok(());
    }

    tracing::info!(
        customer_id,
        pi_id,
        "auto top-up: SCA recovery handoff staged"
    );
    Ok(())
}

async fn fetch_customer_email(
    stripe: &StripeClient,
    customer_id: &str,
) -> anyhow::Result<Option<String>> {
    let resp = stripe
        .get(&format!("customers/{customer_id}"), &[])
        .await
        .context("fetch Stripe customer for email")?;
    Ok(resp
        .body
        .get("email")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// Pull `amount` + saved card's `brand` + `last4` for an SCA-pending PI
/// so the recovery email can show the user exactly what they're being
/// asked to authorise. Returns empty strings on any failure — the email
/// caller falls back to a minimal subject line so a transient Stripe
/// lookup never blocks the recovery flow.
async fn fetch_pi_details(stripe: &StripeClient, pi_id: &str) -> Option<(String, String, String)> {
    let resp = stripe
        .get(
            &format!("payment_intents/{pi_id}"),
            &[("expand[]", "payment_method")],
        )
        .await
        .ok()?;
    let amount_cents = resp
        .body
        .get("amount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let amount_dollars = if amount_cents > 0 {
        format!("{:.2}", amount_cents as f64 / 100.0)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        String::new()
    };
    let brand = resp
        .body
        .pointer("/payment_method/card/brand")
        .and_then(|v| v.as_str())
        .map(prettify_brand)
        .unwrap_or_default();
    let last4 = resp
        .body
        .pointer("/payment_method/card/last4")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some((amount_dollars, brand, last4))
}

/// Render a Stripe card brand identifier ("visa", "mastercard") in
/// human-friendly title case. Unknown brands keep their original
/// (lowercased) form. Mirrors the dashboard's `prettyBrand` helper.
fn prettify_brand(brand: &str) -> String {
    match brand.to_ascii_lowercase().as_str() {
        "visa" => "Visa".to_string(),
        "mastercard" => "Mastercard".to_string(),
        "amex" => "Amex".to_string(),
        "discover" => "Discover".to_string(),
        "diners" => "Diners".to_string(),
        "jcb" => "JCB".to_string(),
        "unionpay" => "UnionPay".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Best-effort fallback extractor — only used by unit tests now.
/// Production code reads `error.payment_intent.id` directly via the
/// `StripeError` downcast (Codex P1 #1 fix).
#[cfg(test)]
fn extract_pi_id(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let needle = b"pi_";
    bytes.windows(3).enumerate().find_map(|(i, w)| {
        if w != needle {
            return None;
        }
        let mut end = i + 3;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end - i >= 6 {
            Some(String::from_utf8_lossy(&bytes[i..end]).into_owned())
        } else {
            None
        }
    })
}

async fn invalidate_cache(cfg: &Config, customer_id: &str) -> anyhow::Result<()> {
    let client = internal_client::build_client()?;
    let body = serde_json::json!({ "customer_id": customer_id });
    internal_client::post_internal(&client, cfg, "/internal/invalidate_balance_cache", &body).await
}

/// Used by `Rocket::custom` configurations that need to expose the
/// signature header on outbound responses for ergonomic debugging in
/// development; not used in production. Kept here so it lives next to
/// the verifier.
#[allow(dead_code)]
fn debug_header(name: &str, value: &str) -> Header<'static> {
    Header::new(name.to_string(), value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_start_unix_is_first_of_month_midnight() {
        let mid_month = OffsetDateTime::new_utc(
            time::Date::from_calendar_date(2026, time::Month::June, 15).unwrap(),
            time::Time::from_hms(12, 34, 56).unwrap(),
        );
        let start = month_start_unix(mid_month);
        let parsed = OffsetDateTime::from_unix_timestamp(start).unwrap();
        assert_eq!(parsed.year(), 2026);
        assert_eq!(parsed.month(), time::Month::June);
        assert_eq!(parsed.day(), 1);
        assert_eq!(parsed.hour(), 0);
        assert_eq!(parsed.minute(), 0);
    }

    #[test]
    fn count_consecutive_failures_stops_at_first_success() {
        let pi = |status: &str, code: Option<&str>| {
            let mut m = serde_json::Map::new();
            m.insert("status".into(), serde_json::Value::String(status.into()));
            if let Some(c) = code {
                m.insert("last_payment_error".into(), serde_json::json!({"code": c}));
            }
            Value::Object(m)
        };
        let list = vec![
            pi("requires_payment_method", None),
            pi("succeeded", None),
            pi("requires_payment_method", None),
            pi("requires_payment_method", None),
        ];
        // Only the first one counts; chain stops at the succeeded.
        assert_eq!(count_consecutive_failures(&list), 1);
    }

    #[test]
    fn count_consecutive_failures_recognizes_error_codes() {
        let pi = |code: &str| serde_json::json!({"status":"processing","last_payment_error":{"code":code}});
        let list = vec![
            pi("card_declined"),
            pi("insufficient_funds"),
            pi("expired_card"),
        ];
        assert_eq!(count_consecutive_failures(&list), 3);
    }

    #[test]
    fn month_spend_sums_non_failed_only() {
        let succeeded = serde_json::json!({"status":"succeeded","amount":2000});
        let failed = serde_json::json!({"status":"requires_payment_method","amount":500});
        let pending = serde_json::json!({"status":"processing","amount":1500});
        let list = vec![succeeded, failed, pending];
        assert_eq!(month_spend_cents(&list), 3500);
    }

    /// Codex P1 (Phase 5): `requires_action` PIs are abandoned SCA
    /// challenges, not real spend. They must NOT count against the
    /// monthly cap; otherwise a single 3DS drop-off can exhaust the
    /// user's budget without a successful charge.
    #[test]
    fn month_spend_excludes_requires_action() {
        let succeeded = serde_json::json!({"status":"succeeded","amount":2000});
        let abandoned = serde_json::json!({"status":"requires_action","amount":5000});
        let list = vec![succeeded, abandoned];
        assert_eq!(month_spend_cents(&list), 2000);
    }

    #[test]
    fn extract_pi_id_pulls_from_error_strings() {
        let s = "Stripe error: authentication_required pi_3TfxzqGhjEGDNSRy0VYfHVf6 details";
        assert_eq!(
            extract_pi_id(s),
            Some("pi_3TfxzqGhjEGDNSRy0VYfHVf6".to_string())
        );
    }

    #[test]
    fn extract_pi_id_returns_none_when_absent() {
        assert!(extract_pi_id("nothing here").is_none());
    }

    /// Plan §6 step 7: at exactly 3 consecutive failures we auto-disable.
    /// One less should NOT trigger.
    #[test]
    fn failure_threshold_is_three() {
        assert_eq!(FAILURE_DISABLE_THRESHOLD, 3);
    }

    /// Plan §6 step 8 / dashboard: floor matches the manual-topup floor.
    #[test]
    fn min_topup_matches_manual_floor() {
        assert_eq!(MIN_TOPUP_CENTS, 500);
    }
}
