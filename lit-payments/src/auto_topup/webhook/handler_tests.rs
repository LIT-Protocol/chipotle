//! Phase 5 integration tests — `POST /stripe/webhook`.
//!
//! These hit **real Stripe test mode** and the local Postgres DB.
//! Silent-skipped when `STRIPE_SECRET_KEY` or `DATABASE_URL` is missing.
//! Each test uses a deterministic wallet so reruns are idempotent.
//!
//! Coverage — paired with the 11 codex gaps in the test-strategy
//! discussion:
//!   - signature tampering → 401  (gap #4 partial)
//!   - timestamp skew → 401  (gap #4)
//!   - non-`customer.updated` event → 200 no-op  (cheap reject)
//!   - `customer.updated` without `previous_attributes.balance` → 200
//!     no-op  (gap #3)
//!   - `enabled=false` config → 200 no Stripe call, no credit row
//!   - balance still at/above threshold → 200 no PI created
//!   - happy path: balance dropped below threshold → PI created → credit
//!     row inserted with non-null `stripe_balance_transaction_id`
//!     (gap #1 partial — see "split-credit" deferred to Phase 6 reconciler
//!     coverage)
//!   - **replay safety**: deliver the same event twice → still exactly
//!     one credit row, balance not double-credited  (gap #2, #6)
//!   - cap reached: pre-existing PIs already at/above cap →  second
//!     top-up skipped  (gap #7, partial of gap #11)

use std::sync::Arc;

use hmac::{Hmac, Mac};
use lit_billing_core::StripeClient;
use rocket::http::{Header, Status};
use rocket::local::asynchronous::Client;
use rocket::{Rocket, routes};
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::PgPool;

use crate::auto_topup::types::AutoTopupConfigUpsert;
use crate::auto_topup::webhook::mutex::PerCustomerMutex;
use crate::config::Config;

type HmacSha256 = Hmac<Sha256>;

const WEBHOOK_SECRET: &str = "whsec_test_phase5_webhook_secret_value_for_integration_tests";

const TEST_WALLET_WEBHOOK_HAPPY: &str = "0x1010101010101010101010101010101010101010";
const TEST_WALLET_WEBHOOK_REPLAY: &str = "0x2020202020202020202020202020202020202020";
const TEST_WALLET_WEBHOOK_CAP: &str = "0x3030303030303030303030303030303030303030";
const TEST_WALLET_WEBHOOK_DISABLED: &str = "0x4040404040404040404040404040404040404040";
const TEST_WALLET_WEBHOOK_BENIGN: &str = "0x5050505050505050505050505050505050505050";
const TEST_WALLET_WEBHOOK_PENDING: &str = "0x6161616161616161616161616161616161616161";

fn stripe_key() -> Option<String> {
    std::env::var("STRIPE_SECRET_KEY").ok().filter(|k| {
        !k.trim().is_empty() && (k.starts_with("rk_test_") || k.starts_with("sk_test_"))
    })
}

fn db_url() -> Option<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
}

fn test_config(stripe_secret_key: String, db_url: String) -> Config {
    Config {
        database_url: db_url,
        magic_link_signing_key: vec![0u8; 32],
        resend_api_key: "unused".into(),
        mail_from: "unused".into(),
        public_base_url: "http://unused".into(),
        stripe_secret_key,
        stripe_publishable_key: "pk_test_phase5".into(),
        max_grant_cents: 0,
        max_daily_per_operator_cents: 0,
        litkey_discount_basis_points: 0,
        litkey_chain: None,
        // Cache-invalidation hop won't be reachable in tests; the
        // fire-and-forget call swallows the error, so a bad URL is fine.
        lit_api_server_base_url: "http://127.0.0.1:1".into(),
        lit_internal_shared_secret: "unused".into(),
        lit_accounts_rpc_url: "http://localhost:8545".to_string(),
        lit_accounts_chain_id: 175188,
        lit_accounts_contract_address: alloy_primitives::Address::ZERO,
        stripe_webhook_secret: WEBHOOK_SECRET.into(),
        reconciler_interval_secs: 900,
        enterprise_billing_interval_secs: 3600,
        stripe_dashboard_base: "https://dashboard.stripe.com".to_string(),
        cors_allowed_origins: vec!["http://localhost".to_string()],
    }
}

async fn build_client(key: String, url: String) -> Client {
    let cfg = test_config(key.clone(), url.clone());
    let stripe = StripeClient::new(key).expect("stripe client");
    let pool = PgPool::connect(&url).await.expect("connect db");
    let rocket = Rocket::build()
        .manage(cfg)
        .manage(stripe)
        .manage(pool)
        .manage(PerCustomerMutex::new())
        .mount("/", routes![super::handler::stripe_webhook]);
    Client::tracked(rocket).await.expect("rocket client")
}

fn sign_now(body: &[u8]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sign_with_timestamp(body, now)
}

fn sign_with_timestamp(body: &[u8], timestamp: i64) -> String {
    let mut mac = HmacSha256::new_from_slice(WEBHOOK_SECRET.as_bytes()).expect("hmac key");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("t={timestamp},v1={sig}")
}

/// Build a `customer.updated` event body with a balance change. Caller
/// supplies the customer id and the new/previous balances.
fn balance_change_event(customer_id: &str, new_balance: i64, previous_balance: i64) -> Vec<u8> {
    let evt = json!({
        "id": "evt_test_webhook",
        "type": "customer.updated",
        "data": {
            "object": {
                "id": customer_id,
                "balance": new_balance,
            },
            "previous_attributes": {
                "balance": previous_balance,
            }
        }
    });
    serde_json::to_vec(&evt).unwrap()
}

/// Build a `customer.updated` event with NO balance change. Mirrors what
/// Stripe sends when only email / metadata changes.
fn metadata_change_event(customer_id: &str) -> Vec<u8> {
    let evt = json!({
        "id": "evt_test_meta",
        "type": "customer.updated",
        "data": {
            "object": {
                "id": customer_id,
                "balance": 0,
            },
            "previous_attributes": {
                "email": "old@example.com",
            }
        }
    });
    serde_json::to_vec(&evt).unwrap()
}

/// Drop the customer balance to the desired value via balance_transactions.
/// Stripe stores customer.balance as a signed integer; negative = credit
/// available. We use `delta = desired - current` to land at `desired`.
async fn set_customer_balance(stripe: &StripeClient, customer_id: &str, desired: i64) {
    let cur = lit_billing_core::balance::fetch(stripe, customer_id)
        .await
        .unwrap_or(0);
    let delta = desired - cur;
    if delta == 0 {
        return;
    }
    let amount = delta.to_string();
    stripe
        .post(
            &format!("customers/{customer_id}/balance_transactions"),
            &[
                ("amount", amount.as_str()),
                ("currency", "usd"),
                ("description", "phase5_test_balance_setter"),
            ],
        )
        .await
        .expect("balance setter");
}

/// Wipe DB rows for the given wallet/customer so each test starts clean.
async fn reset_for(pool: &PgPool, wallet: &str, customer_id: &str) {
    let _ = sqlx::query("DELETE FROM auto_topup_credits WHERE customer_id = $1")
        .bind(customer_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM auto_topup_config WHERE wallet_address = $1")
        .bind(wallet)
        .execute(pool)
        .await;
}

/// Seed an `enabled = true` config row with default knobs and the given pm.
async fn seed_enabled_config(
    pool: &PgPool,
    customer_id: &str,
    wallet: &str,
    pm_id: &str,
    threshold: i64,
    topup: i64,
    cap: i64,
) {
    let upsert = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: Some(threshold),
        topup_amount_cents: Some(topup),
        monthly_cap_cents: Some(cap),
        payment_method_id: Some(pm_id.to_string()),
        consent_version: Some("v1".into()),
    };
    crate::auto_topup::db::upsert(pool, customer_id, wallet, &upsert)
        .await
        .expect("seed config");
}

async fn count_credit_rows(pool: &PgPool, customer_id: &str) -> i64 {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM auto_topup_credits WHERE customer_id = $1")
            .bind(customer_id)
            .fetch_one(pool)
            .await
            .unwrap();
    n
}

// ──────────────────────────────────────────────────────────────────────────
// Signature / event-filter tests (no Stripe network calls)
// ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial_test::serial]
async fn rejects_tampered_signature() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url).await;
    let body = balance_change_event("cus_unused", -100, 0);
    let header = sign_now(&body);
    // Mutate one hex char in the v1= portion.
    let bad = header.replace(",v1=", ",v1=ff");
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", bad))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Unauthorized);
}

#[tokio::test]
#[serial_test::serial]
async fn rejects_stale_timestamp() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url).await;
    let body = balance_change_event("cus_unused", -100, 0);
    let stale = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 3600; // 1 hour ago
    let header = sign_with_timestamp(&body, stale);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Unauthorized);
}

#[tokio::test]
#[serial_test::serial]
async fn ignores_event_without_balance_change() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url).await;
    let body = metadata_change_event("cus_anything");
    let header = sign_now(&body);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    // No balance change → handler treats as not-our-event → 200.
    assert_eq!(resp.status(), Status::Ok);
}

#[tokio::test]
#[serial_test::serial]
async fn ignores_non_customer_updated_event() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url).await;
    let body = serde_json::to_vec(&json!({
        "type": "charge.succeeded",
        "data": {"object": {"id": "ch_xxx"}}
    }))
    .unwrap();
    let header = sign_now(&body);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
}

// ──────────────────────────────────────────────────────────────────────────
// Config-state short-circuit tests (Stripe customer, no PI creation)
// ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial_test::serial]
async fn short_circuits_when_config_disabled() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_WEBHOOK_DISABLED,
    )
    .await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_WEBHOOK_DISABLED, &cust).await;

    // Disabled config: just an enabled=false row exists for this wallet.
    let upsert = AutoTopupConfigUpsert {
        enabled: false,
        threshold_cents: None,
        topup_amount_cents: None,
        monthly_cap_cents: None,
        payment_method_id: None,
        consent_version: None,
    };
    crate::auto_topup::db::upsert(&pool, &cust, TEST_WALLET_WEBHOOK_DISABLED, &upsert)
        .await
        .unwrap();

    let client = build_client(key, url).await;
    let body = balance_change_event(&cust, -100, 0);
    let header = sign_now(&body);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    assert_eq!(count_credit_rows(&pool, &cust).await, 0);
}

#[tokio::test]
#[serial_test::serial]
async fn ignores_when_payload_balance_above_threshold() {
    // Payload claims balance still has plenty of credit (-2000 = $20).
    // Even though the row is enabled with threshold=500 ($5), no top-up
    // fires because the user isn't actually low.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_WEBHOOK_BENIGN,
    )
    .await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_WEBHOOK_BENIGN, &cust).await;

    let pm_id = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    seed_enabled_config(
        &pool,
        &cust,
        TEST_WALLET_WEBHOOK_BENIGN,
        &pm_id,
        500,
        2_000,
        10_000,
    )
    .await;

    let client = build_client(key, url).await;
    let body = balance_change_event(&cust, -5_000, -10_000); // still $50 credit
    let header = sign_now(&body);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    assert_eq!(count_credit_rows(&pool, &cust).await, 0);
}

// ──────────────────────────────────────────────────────────────────────────
// Happy path + replay safety (real Stripe charge fires)
// ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial_test::serial]
async fn happy_path_charges_and_credits() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_WEBHOOK_HAPPY,
    )
    .await;
    let pm_id = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_WEBHOOK_HAPPY, &cust).await;
    seed_enabled_config(
        &pool,
        &cust,
        TEST_WALLET_WEBHOOK_HAPPY,
        &pm_id,
        500,
        2_000,
        10_000,
    )
    .await;
    // Drive the actual Stripe balance below threshold so the handler's
    // fresh re-fetch (step 5) agrees with the payload.
    set_customer_balance(&stripe, &cust, -100).await; // $1 credit, below $5

    let client = build_client(key.clone(), url.clone()).await;
    let body = balance_change_event(&cust, -100, -1000);
    let header = sign_now(&body);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(
        resp.status(),
        Status::Ok,
        "body: {}",
        resp.into_string().await.unwrap_or_default()
    );
    assert_eq!(count_credit_rows(&pool, &cust).await, 1);

    // Verify the credit row has a non-null balance_transaction_id (full
    // credit committed, not partial).
    let (bt_id,): (Option<String>,) = sqlx::query_as(
        "SELECT stripe_balance_transaction_id FROM auto_topup_credits WHERE customer_id = $1",
    )
    .bind(&cust)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(bt_id.is_some(), "balance_transaction_id should be set");
}

#[tokio::test]
#[serial_test::serial]
async fn replay_of_same_event_is_safe() {
    // Two deliveries of the same customer.updated event must not result
    // in two credits — protects against Stripe webhook redelivery /
    // network blip retries.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_WEBHOOK_REPLAY,
    )
    .await;
    let pm_id = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_WEBHOOK_REPLAY, &cust).await;
    seed_enabled_config(
        &pool,
        &cust,
        TEST_WALLET_WEBHOOK_REPLAY,
        &pm_id,
        500,
        2_000,
        10_000,
    )
    .await;
    set_customer_balance(&stripe, &cust, -100).await;

    let client = build_client(key.clone(), url.clone()).await;
    let body = balance_change_event(&cust, -100, -1000);
    let header = sign_now(&body);

    // Deliver the event twice.
    let r1 = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header.clone()))
        .body(body.clone())
        .dispatch()
        .await;
    assert_eq!(r1.status(), Status::Ok);
    let credits_after_first = count_credit_rows(&pool, &cust).await;

    // On the second delivery, the fresh balance fetch (step 5) sees the
    // balance is now ABOVE threshold (we just credited $20), so the
    // handler short-circuits without creating a new PI. This is the
    // expected real-world behaviour — Stripe's stale webhook re-delivery
    // arrives after the credit has settled. The credit count must not
    // increase regardless.
    let r2 = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(r2.status(), Status::Ok);
    let credits_after_second = count_credit_rows(&pool, &cust).await;
    assert_eq!(
        credits_after_first, credits_after_second,
        "replay must not create an additional credit row"
    );
    assert_eq!(credits_after_first, 1);
}

// ──────────────────────────────────────────────────────────────────────────
// Cap reached
// ──────────────────────────────────────────────────────────────────────────

/// Helper: sum existing this-month auto_topup PI amounts at Stripe for
/// this customer. Lets the cap test set a budget that leaves room for
/// exactly one more top-up regardless of accumulated state from prior
/// test runs.
async fn existing_month_spend(stripe: &StripeClient, customer_id: &str) -> i64 {
    let month_start = {
        let now = time::OffsetDateTime::now_utc();
        time::OffsetDateTime::new_utc(
            time::Date::from_calendar_date(now.year(), now.month(), 1).unwrap(),
            time::Time::MIDNIGHT,
        )
        .unix_timestamp()
    };
    let since = month_start.to_string();
    let resp = stripe
        .get(
            "payment_intents",
            &[
                ("customer", customer_id),
                ("limit", "100"),
                ("created[gte]", since.as_str()),
            ],
        )
        .await
        .expect("list pis");
    resp.body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|pi| {
                    let is_auto = pi.pointer("/metadata/source").and_then(|v| v.as_str())
                        == Some("auto_topup");
                    let status = pi.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    let failed = status == "requires_payment_method";
                    is_auto && !failed
                })
                .filter_map(|pi| pi.get("amount").and_then(|v| v.as_i64()))
                .sum()
        })
        .unwrap_or(0)
}

#[tokio::test]
#[serial_test::serial]
async fn cap_reached_skips_charge() {
    // Sum of this-month auto_topup PIs already equals the cap. Webhook
    // arrives with balance below threshold but the cap check refuses.
    //
    // State-aware setup: Stripe customers carry PI history across runs
    // and we can't delete past PIs. So compute the existing this-month
    // auto_topup spend at Stripe and set the cap to `existing + topup` —
    // that leaves room for *exactly one more* charge, regardless of how
    // many times the test has run before.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_WEBHOOK_CAP,
    )
    .await;
    let pm_id = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_WEBHOOK_CAP, &cust).await;

    let topup = 500i64; // $5 per top-up
    let existing = existing_month_spend(&stripe, &cust).await;
    // cap = existing + topup → there is room for the first delivery and
    // exactly one more, so the SECOND delivery should be the one refused.
    let cap = existing + topup;
    seed_enabled_config(
        &pool,
        &cust,
        TEST_WALLET_WEBHOOK_CAP,
        &pm_id,
        500,
        topup,
        cap,
    )
    .await;
    set_customer_balance(&stripe, &cust, -100).await;

    let client = build_client(key.clone(), url.clone()).await;

    let credits_before = count_credit_rows(&pool, &cust).await;

    // First delivery: should charge $5 (fills cap exactly).
    let body = balance_change_event(&cust, -100, -1000);
    let header = sign_now(&body);
    let r1 = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(r1.status(), Status::Ok);
    let credits_after_first = count_credit_rows(&pool, &cust).await;
    assert_eq!(
        credits_after_first,
        credits_before + 1,
        "first delivery should have credited the user"
    );

    // Second delivery: cap is now full → must be skipped.
    set_customer_balance(&stripe, &cust, -100).await;
    let body2 = balance_change_event(&cust, -100, -1000);
    let header2 = sign_now(&body2);
    let r2 = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header2))
        .body(body2)
        .dispatch()
        .await;
    assert_eq!(r2.status(), Status::Ok);
    assert_eq!(
        count_credit_rows(&pool, &cust).await,
        credits_after_first,
        "cap should have prevented a second credit"
    );
}

/// Codex P1 #2: while an SCA-pending PI is in flight, another
/// `customer.updated` arriving for the same customer MUST NOT spawn a
/// second off-session PI. Doing so would overwrite the single-use
/// recovery_token and leave the user stranded between two pending PIs.
///
/// Seed: enabled config row with `pending_action_pi_id` populated (the
/// state the webhook handler would have written when the first PI hit
/// `authentication_required`). Then deliver a balance-drop event and
/// assert no new credit row appears.
#[tokio::test]
#[serial_test::serial]
async fn pending_action_pauses_further_topups() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_WEBHOOK_PENDING,
    )
    .await;
    let pm_id = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_WEBHOOK_PENDING, &cust).await;
    seed_enabled_config(
        &pool,
        &cust,
        TEST_WALLET_WEBHOOK_PENDING,
        &pm_id,
        500,
        2_000,
        10_000,
    )
    .await;
    // Simulate the prior webhook tick having staged an SCA handoff.
    crate::auto_topup::db::set_pending_action(
        &pool,
        &cust,
        "pi_test_pending_blockade_xxx",
        "phase5-pending-blockade-token",
    )
    .await
    .unwrap();
    set_customer_balance(&stripe, &cust, -100).await;

    let client = build_client(key.clone(), url.clone()).await;
    let body = balance_change_event(&cust, -100, -1000);
    let header = sign_now(&body);
    let resp = client
        .post("/stripe/webhook")
        .header(Header::new("Stripe-Signature", header))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    assert_eq!(
        count_credit_rows(&pool, &cust).await,
        0,
        "pending SCA must block additional off-session charges"
    );
    // The recovery token must still be intact — the short-circuit fired
    // before any DB mutation could overwrite it.
    let (token,): (Option<String>,) =
        sqlx::query_as("SELECT recovery_token FROM auto_topup_config WHERE customer_id = $1")
            .bind(&cust)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(token.as_deref(), Some("phase5-pending-blockade-token"));
}
