//! Phase 6 integration tests for the reconciler.
//!
//! Tests exercise the three triage branches of `reconcile_customer`:
//!   - Row exists with non-null `stripe_balance_transaction_id` → skip.
//!   - Row exists with NULL `stripe_balance_transaction_id` → reconciler
//!     re-runs `balance_transactions` (idempotency key suppresses dupes
//!     at Stripe) and fills in the row.
//!   - No row at all → reconciler INSERTs the credit row AND writes the
//!     balance_transaction.
//!
//! Each test drives a real Stripe PI to `succeeded` by creating one
//! synchronously off-session against an attached test card, then
//! manipulates DB state to simulate the failure mode.

use lit_billing_core::StripeClient;
use serde_json::Value;
use sqlx::PgPool;

use crate::config::Config;

const TEST_WALLET_RECONCILER_FULL: &str = "0x6060606060606060606060606060606060606060";
const TEST_WALLET_RECONCILER_PARTIAL: &str = "0x7070707070707070707070707070707070707070";
const TEST_WALLET_RECONCILER_SKIP: &str = "0x8080808080808080808080808080808080808080";

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
        stripe_publishable_key: "pk_test_phase6".into(),
        max_grant_cents: 0,
        max_daily_per_operator_cents: 0,
        litkey_discount_basis_points: 0,
        litkey_chain: None,
        lit_api_server_base_url: "http://127.0.0.1:1".into(),
        lit_internal_shared_secret: "unused".into(),
        lit_accounts_rpc_url: "http://localhost:8545".to_string(),
        lit_accounts_chain_id: 175188,
        lit_accounts_contract_address: alloy_primitives::Address::ZERO,
        stripe_webhook_secret: "unused".into(),
        reconciler_interval_secs: 60,
        cors_allowed_origins: vec!["http://localhost".to_string()],
        gas_funder: None,
    }
}

/// Create a real auto_topup PaymentIntent in `succeeded` status by
/// off-session-confirming against a test Visa. Returns (pi_id, amount).
async fn create_succeeded_auto_topup_pi(
    stripe: &StripeClient,
    customer_id: &str,
    pm_id: &str,
    wallet: &str,
    amount_cents: i64,
) -> (String, i64) {
    let amount_str = amount_cents.to_string();
    let resp = stripe
        .post(
            "payment_intents",
            &[
                ("amount", amount_str.as_str()),
                ("currency", "usd"),
                ("customer", customer_id),
                ("payment_method", pm_id),
                ("off_session", "true"),
                ("confirm", "true"),
                ("metadata[source]", "auto_topup"),
                ("metadata[wallet_address]", wallet),
            ],
        )
        .await
        .expect("create PI");
    let pi_id = resp.body["id"].as_str().unwrap().to_string();
    let status = resp.body["status"].as_str().unwrap();
    assert_eq!(status, "succeeded", "expected succeeded PI, got {status}");
    (pi_id, amount_cents)
}

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

async fn seed_enabled(pool: &PgPool, customer_id: &str, wallet: &str, pm_id: &str) {
    use crate::auto_topup::types::AutoTopupConfigUpsert;
    let upsert = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: Some(500),
        topup_amount_cents: Some(500),
        monthly_cap_cents: Some(10_000),
        payment_method_id: Some(pm_id.into()),
        consent_version: Some("v1".into()),
    };
    crate::auto_topup::db::upsert(pool, customer_id, wallet, &upsert)
        .await
        .unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_credits_orphan_pi_with_no_row() {
    // Webhook handler crashed before INSERT. Reconciler must INSERT the
    // credit row AND write the balance_transaction.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_RECONCILER_FULL,
    )
    .await;
    let pm = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RECONCILER_FULL, &cust).await;
    seed_enabled(&pool, &cust, TEST_WALLET_RECONCILER_FULL, &pm).await;

    let (pi_id, amount) =
        create_succeeded_auto_topup_pi(&stripe, &cust, &pm, TEST_WALLET_RECONCILER_FULL, 500).await;

    let before: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM auto_topup_credits WHERE payment_intent_id = $1")
            .bind(&pi_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before.0, 0);

    let cfg = test_config(key.clone(), url.clone());
    super::reconciler::run_once(&cfg, &stripe, &pool)
        .await
        .expect("reconcile");

    let (row, bt): (i32, Option<String>) = sqlx::query_as(
        "SELECT 1, stripe_balance_transaction_id FROM auto_topup_credits \
         WHERE payment_intent_id = $1",
    )
    .bind(&pi_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, 1, "orphan PI should have been credited");
    assert!(bt.is_some(), "balance_tx must be set after reconciler");
    let _ = amount;
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_completes_partial_credit() {
    // Webhook handler INSERTed the credit row but the balance_tx write
    // never landed. Reconciler must retry that write (same idempotency
    // key) and fill in stripe_balance_transaction_id.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_RECONCILER_PARTIAL,
    )
    .await;
    let pm = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RECONCILER_PARTIAL, &cust).await;
    seed_enabled(&pool, &cust, TEST_WALLET_RECONCILER_PARTIAL, &pm).await;

    let (pi_id, amount) =
        create_succeeded_auto_topup_pi(&stripe, &cust, &pm, TEST_WALLET_RECONCILER_PARTIAL, 500)
            .await;
    // Simulate the "partial credit" state: row inserted, bt_id null.
    // Backdate `credited_at` past the reconciler's MIN_PARTIAL_AGE_SECS
    // (60s) age gate — fresh partials are intentionally skipped because
    // they're overwhelmingly likely to be the live webhook's in-flight
    // balance_tx write. An integration test simulating a crashed webhook
    // is exactly the "real orphan" case, so we backdate to 5 minutes
    // ago so the reconciler picks it up.
    sqlx::query(
        "INSERT INTO auto_topup_credits \
            (payment_intent_id, customer_id, amount_cents, credited_at) \
         VALUES ($1, $2, $3, now() - interval '5 minutes')",
    )
    .bind(&pi_id)
    .bind(&cust)
    .bind(amount)
    .execute(&pool)
    .await
    .unwrap();

    let cfg = test_config(key.clone(), url.clone());
    super::reconciler::run_once(&cfg, &stripe, &pool)
        .await
        .expect("reconcile");

    let (bt,): (Option<String>,) = sqlx::query_as(
        "SELECT stripe_balance_transaction_id FROM auto_topup_credits \
         WHERE payment_intent_id = $1",
    )
    .bind(&pi_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        bt.is_some(),
        "reconciler must fill in stripe_balance_transaction_id"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_skips_already_completed() {
    // Row exists with non-null bt_id; reconciler must not touch it.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust = crate::billing::setup_intent_tests::ensure_unique_customer(
        &stripe,
        TEST_WALLET_RECONCILER_SKIP,
    )
    .await;
    let pm = crate::billing::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RECONCILER_SKIP, &cust).await;
    seed_enabled(&pool, &cust, TEST_WALLET_RECONCILER_SKIP, &pm).await;

    let (pi_id, amount) =
        create_succeeded_auto_topup_pi(&stripe, &cust, &pm, TEST_WALLET_RECONCILER_SKIP, 500).await;
    sqlx::query(
        "INSERT INTO auto_topup_credits (payment_intent_id, customer_id, amount_cents, stripe_balance_transaction_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&pi_id)
    .bind(&cust)
    .bind(amount)
    .bind("bt_already_credited_via_test_seed")
    .execute(&pool)
    .await
    .unwrap();

    let cfg = test_config(key.clone(), url.clone());
    super::reconciler::run_once(&cfg, &stripe, &pool)
        .await
        .expect("reconcile");

    // bt_id should still be the test seed value — reconciler did not touch.
    let (bt,): (Option<String>,) = sqlx::query_as(
        "SELECT stripe_balance_transaction_id FROM auto_topup_credits \
         WHERE payment_intent_id = $1",
    )
    .bind(&pi_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        bt.as_deref(),
        Some("bt_already_credited_via_test_seed"),
        "reconciler must not overwrite completed rows"
    );
    // Avoid unused-variable lint on `amount` and `_` patterns.
    let _ = Value::Null;
}
