//! Integration tests for the LITKEY partial-credit reconciler (CPL-375).
//!
//! Each test seeds a `litkey_payments` row in the "partial" state a real
//! claim would leave after a crash between the INSERT and the Stripe
//! `balance_transactions` write (`status='credited'`,
//! `stripe_balance_transaction_id IS NULL`), then drives `run_once` and
//! asserts the age-gated outcome.
//!
//! Gated on `DATABASE_URL` + a Stripe test key; the happy-path test issues a
//! real `balance_transactions` write against a test customer. Tests return
//! early (skip) when either is absent, matching the auto_topup reconciler
//! tests.

use lit_billing_core::StripeClient;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

const TEST_WALLET: &str = "0x9090909090909090909090909090909090909090";
const TEST_PAYER: &str = "0x000000000000000000000000000000000000dead";
const GATEWAY: &str = super::chain::DEFAULT_GATEWAY_ADDRESS;

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

/// Insert a partial credited row (`stripe_balance_transaction_id` NULL) for
/// a distinct `tx_hash`, back-dating `credited_at` by `age`. Returns the
/// tx_hash used.
async fn seed_partial(
    pool: &PgPool,
    customer_id: &str,
    tx_hash: &str,
    log_index: i64,
    age: Duration,
) {
    sqlx::query("DELETE FROM litkey_payments WHERE tx_hash = $1")
        .bind(tx_hash)
        .execute(pool)
        .await
        .unwrap();
    let credited_at = OffsetDateTime::now_utc() - age;
    sqlx::query(
        "INSERT INTO litkey_payments (
            chain_id, gateway_address, tx_hash, log_index, block_number,
            wallet_address, payer_address, litkey_amount_wei, usd_wei_per_litkey,
            discount_basis_points, cents_credited, status, stripe_customer_id,
            stripe_balance_transaction_id, credited_at
         ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8::numeric, $9::numeric,
            $10, $11, 'credited', $12,
            NULL, $13
         )",
    )
    .bind(super::chain::BASE_CHAIN_ID)
    .bind(GATEWAY)
    .bind(tx_hash)
    .bind(log_index)
    .bind(1_000_000i64)
    .bind(TEST_WALLET)
    .bind(TEST_PAYER)
    .bind("1000000000000000000")
    .bind("1000000000000000000")
    .bind(2_000i64)
    .bind(125i64)
    .bind(customer_id)
    .bind(credited_at)
    .execute(pool)
    .await
    .expect("seed partial litkey_payments row");
}

async fn balance_tx_id(pool: &PgPool, tx_hash: &str) -> Option<String> {
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT stripe_balance_transaction_id FROM litkey_payments WHERE tx_hash = $1",
    )
    .bind(tx_hash)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_completes_partial_credit() {
    // Crash after INSERT, before the balance_transactions write. The
    // reconciler must replay the credit and fill in the balance_tx id.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key).unwrap();
    let cust =
        crate::billing::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET).await;
    let pool = PgPool::connect(&url).await.unwrap();

    let tx_hash = "0x1111111111111111111111111111111111111111111111111111111111111111";
    seed_partial(&pool, &cust, tx_hash, 1, Duration::minutes(5)).await;
    assert!(balance_tx_id(&pool, tx_hash).await.is_none());

    super::litkey_reconciler::run_once(&stripe, &pool)
        .await
        .expect("reconcile");

    assert!(
        balance_tx_id(&pool, tx_hash).await.is_some(),
        "partial credit should be completed with a balance_tx id"
    );

    sqlx::query("DELETE FROM litkey_payments WHERE tx_hash = $1")
        .bind(tx_hash)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn claim_path_completes_partial_credit_synchronously() {
    // The synchronous claim-path primitives: find the partial for a
    // (tx_hash, wallet) and complete it inline. A fresh partial (no min-age
    // gate) must still complete.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key).unwrap();
    let cust =
        crate::billing::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET).await;
    let pool = PgPool::connect(&url).await.unwrap();

    let tx_hash = "0x4444444444444444444444444444444444444444444444444444444444444444";
    seed_partial(&pool, &cust, tx_hash, 4, Duration::seconds(1)).await;

    let partial = super::chain::find_partial_litkey_credit(&pool, tx_hash, TEST_WALLET)
        .await
        .unwrap()
        .expect("a partial credit should be found for the tx/wallet");
    super::chain::complete_partial_litkey_credit(&stripe, &pool, &partial)
        .await
        .expect("complete partial");

    assert!(
        balance_tx_id(&pool, tx_hash).await.is_some(),
        "synchronous completion should fill in the balance_tx id"
    );
    // Once completed the row is no longer a partial.
    assert!(
        super::chain::find_partial_litkey_credit(&pool, tx_hash, TEST_WALLET)
            .await
            .unwrap()
            .is_none(),
        "completed row must not be returned as a partial"
    );

    sqlx::query("DELETE FROM litkey_payments WHERE tx_hash = $1")
        .bind(tx_hash)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_skips_fresh_partial() {
    // A partial younger than MIN_PARTIAL_AGE_SECS is the live claim still
    // in-flight; the reconciler must leave it alone.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key).unwrap();
    let cust =
        crate::billing::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET).await;
    let pool = PgPool::connect(&url).await.unwrap();

    let tx_hash = "0x2222222222222222222222222222222222222222222222222222222222222222";
    seed_partial(&pool, &cust, tx_hash, 2, Duration::seconds(1)).await;

    super::litkey_reconciler::run_once(&stripe, &pool)
        .await
        .expect("reconcile");

    assert!(
        balance_tx_id(&pool, tx_hash).await.is_none(),
        "a fresh partial must not be touched by the reconciler"
    );

    sqlx::query("DELETE FROM litkey_payments WHERE tx_hash = $1")
        .bind(tx_hash)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_skips_partial_past_idempotency_window() {
    // Past ~24h the Stripe idempotency key no longer dedupes, so the
    // reconciler must NOT replay (it would risk a double-credit) — it logs
    // for manual repair and leaves the row partial.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key).unwrap();
    let cust =
        crate::billing::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET).await;
    let pool = PgPool::connect(&url).await.unwrap();

    let tx_hash = "0x3333333333333333333333333333333333333333333333333333333333333333";
    seed_partial(&pool, &cust, tx_hash, 3, Duration::hours(24)).await;

    super::litkey_reconciler::run_once(&stripe, &pool)
        .await
        .expect("reconcile");

    assert!(
        balance_tx_id(&pool, tx_hash).await.is_none(),
        "a partial past the idempotency window must be left for manual repair"
    );

    sqlx::query("DELETE FROM litkey_payments WHERE tx_hash = $1")
        .bind(tx_hash)
        .execute(&pool)
        .await
        .unwrap();
}
