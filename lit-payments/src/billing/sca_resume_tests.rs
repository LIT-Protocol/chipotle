//! Phase 7 integration tests for the SCA resume endpoints.
//!
//! These hit real Stripe (PaymentIntent retrieve, balance_transactions)
//! and real local Postgres. Silent-skip when STRIPE_SECRET_KEY /
//! DATABASE_URL missing.

use lit_billing_core::StripeClient;
use rocket::http::{ContentType, Status};
use rocket::local::asynchronous::Client;
use rocket::{Rocket, routes};
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::auto_topup::types::AutoTopupConfigUpsert;
use crate::config::Config;

const TEST_WALLET_RESUME_GET: &str = "0x9090909090909090909090909090909090909090";
const TEST_WALLET_RESUME_EXPIRED: &str = "0xa1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const TEST_WALLET_RESUME_COMPLETE: &str = "0xb2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
const TEST_WALLET_RESUME_TRANSIENT: &str = "0xc3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";

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
        stripe_publishable_key: "pk_test_phase7_test".into(),
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
        reconciler_interval_secs: 900,
        cors_allowed_origins: vec!["http://localhost".to_string()],
        gas_funder: None,
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
        .mount(
            "/",
            routes![
                super::sca_resume::get_auto_topup_resume,
                super::sca_resume::post_auto_topup_resume_complete,
            ],
        );
    Client::tracked(rocket).await.expect("rocket client")
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

/// Build a row with the SCA pending state pre-populated. Mirrors what
/// the webhook handler would have written when it hit
/// authentication_required.
async fn seed_pending_row(
    pool: &PgPool,
    customer_id: &str,
    wallet: &str,
    pm_id: &str,
    pi_id: &str,
    token: &str,
    expires_at: OffsetDateTime,
) {
    // First UPSERT a normal enabled row, then overwrite the pending
    // fields directly. (The high-level upsert helper deliberately doesn't
    // accept those — the webhook handler is the only legitimate writer
    // in production.)
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
    sqlx::query(
        "UPDATE auto_topup_config \
            SET pending_action_pi_id = $1, pending_action_at = $2, \
                recovery_token = $3, recovery_token_expires_at = $4, \
                disabled_reason = 'requires_action' \
            WHERE customer_id = $5",
    )
    .bind(pi_id)
    .bind(OffsetDateTime::now_utc())
    .bind(token)
    .bind(expires_at)
    .bind(customer_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Create a real `succeeded` auto_topup PaymentIntent for tests that need
/// one. Used by the `/complete` happy-path test.
async fn create_succeeded_pi(
    stripe: &StripeClient,
    customer_id: &str,
    pm_id: &str,
    wallet: &str,
) -> String {
    let resp = stripe
        .post(
            "payment_intents",
            &[
                ("amount", "500"),
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
        .unwrap();
    assert_eq!(resp.body["status"].as_str().unwrap(), "succeeded");
    resp.body["id"].as_str().unwrap().to_string()
}

/// Create a synthetic PaymentIntent we never confirm. Used for the GET
/// tests which only need a retrievable id; the actual PI status doesn't
/// matter because the GET endpoint just returns the client_secret.
async fn create_unconfirmed_pi(stripe: &StripeClient, customer_id: &str, wallet: &str) -> String {
    let resp = stripe
        .post(
            "payment_intents",
            &[
                ("amount", "500"),
                ("currency", "usd"),
                ("customer", customer_id),
                ("metadata[source]", "auto_topup"),
                ("metadata[wallet_address]", wallet),
            ],
        )
        .await
        .unwrap();
    resp.body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[serial_test::serial]
async fn get_resume_with_valid_token_returns_client_secret() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_RESUME_GET).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RESUME_GET, &cust).await;
    let pm = super::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pi_id = create_unconfirmed_pi(&stripe, &cust, TEST_WALLET_RESUME_GET).await;
    let token = "phase7-test-token-aaa-deterministic";
    let expires = OffsetDateTime::now_utc() + ::time::Duration::hours(1);
    seed_pending_row(
        &pool,
        &cust,
        TEST_WALLET_RESUME_GET,
        &pm,
        &pi_id,
        token,
        expires,
    )
    .await;

    let client = build_client(key.clone(), url.clone()).await;
    let resp = client
        .get(format!("/billing/auto_topup_resume?token={token}"))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let body: Value = resp.into_json().await.unwrap();
    assert_eq!(body["payment_intent_id"], pi_id);
    assert!(
        body["client_secret"]
            .as_str()
            .unwrap_or("")
            .starts_with(&pi_id),
        "client_secret should begin with PI id"
    );

    // Second GET with the same token must 404 (single-use).
    let resp2 = client
        .get(format!("/billing/auto_topup_resume?token={token}"))
        .dispatch()
        .await;
    assert_eq!(resp2.status(), Status::NotFound);
}

#[tokio::test]
#[serial_test::serial]
async fn get_resume_with_expired_token_returns_404() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_RESUME_EXPIRED)
            .await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RESUME_EXPIRED, &cust).await;
    let pm = super::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pi_id = create_unconfirmed_pi(&stripe, &cust, TEST_WALLET_RESUME_EXPIRED).await;
    let token = "phase7-test-token-expired-bbb";
    let already_expired = OffsetDateTime::now_utc() - ::time::Duration::hours(1);
    seed_pending_row(
        &pool,
        &cust,
        TEST_WALLET_RESUME_EXPIRED,
        &pm,
        &pi_id,
        token,
        already_expired,
    )
    .await;

    let client = build_client(key, url).await;
    let resp = client
        .get(format!("/billing/auto_topup_resume?token={token}"))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::NotFound);
}

#[tokio::test]
#[serial_test::serial]
async fn get_resume_with_unknown_token_returns_404() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url).await;
    let resp = client
        .get("/billing/auto_topup_resume?token=never-issued-this-token")
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::NotFound);
}

#[tokio::test]
#[serial_test::serial]
async fn complete_with_succeeded_pi_credits_and_clears_pending() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_RESUME_COMPLETE)
            .await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RESUME_COMPLETE, &cust).await;
    let pm = super::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    let pi_id = create_succeeded_pi(&stripe, &cust, &pm, TEST_WALLET_RESUME_COMPLETE).await;
    // Pretend the webhook handler queued this PI as the SCA-pending one.
    let token = "phase7-test-token-complete-ccc";
    let expires = OffsetDateTime::now_utc() + ::time::Duration::hours(1);
    seed_pending_row(
        &pool,
        &cust,
        TEST_WALLET_RESUME_COMPLETE,
        &pm,
        &pi_id,
        token,
        expires,
    )
    .await;

    let client = build_client(key.clone(), url.clone()).await;
    let body = serde_json::to_string(&serde_json::json!({
        "payment_intent_id": pi_id,
    }))
    .unwrap();
    let resp = client
        .post("/billing/auto_topup_resume/complete")
        .header(ContentType::JSON)
        .body(body)
        .dispatch()
        .await;
    assert_eq!(
        resp.status(),
        Status::Ok,
        "body: {}",
        resp.into_string().await.unwrap_or_default()
    );

    // Credit row inserted with non-null bt_id.
    let (bt,): (Option<String>,) = sqlx::query_as(
        "SELECT stripe_balance_transaction_id FROM auto_topup_credits \
         WHERE payment_intent_id = $1",
    )
    .bind(&pi_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(bt.is_some());

    // Pending state cleared.
    let (pending,): (Option<String>,) =
        sqlx::query_as("SELECT pending_action_pi_id FROM auto_topup_config WHERE customer_id = $1")
            .bind(&cust)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(pending.is_none());
}

#[tokio::test]
#[serial_test::serial]
async fn complete_rejects_non_succeeded_pi() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_RESUME_COMPLETE)
            .await;
    let pi_id = create_unconfirmed_pi(&stripe, &cust, TEST_WALLET_RESUME_COMPLETE).await;
    let client = build_client(key, url).await;
    let body = serde_json::to_string(&serde_json::json!({"payment_intent_id": pi_id})).unwrap();
    let resp = client
        .post("/billing/auto_topup_resume/complete")
        .header(ContentType::JSON)
        .body(body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.unwrap();
    assert_eq!(body["error"], "pi_not_succeeded");
}

/// Codex P2 #3 regression: when the Stripe retrieve fails, the recovery
/// token MUST stay valid so the user can retry. We can't easily inject
/// "Stripe is down" against the real API, so we inject the failure
/// case the resolver actually hits in the wild: a `pending_action_pi_id`
/// that no longer exists at Stripe (deleted in test cleanup, or never
/// created). That triggers `stripe_unavailable` (4xx from Stripe → our
/// error mapping) — pre-fix, the token had already been consumed by the
/// lookup so the next call 404'd. Post-fix, the token survives.
#[tokio::test]
#[serial_test::serial]
async fn get_resume_preserves_token_on_stripe_failure() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_RESUME_TRANSIENT)
            .await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_for(&pool, TEST_WALLET_RESUME_TRANSIENT, &cust).await;
    let pm = super::setup_intent_tests::attach_test_card(&stripe, &cust).await;
    // Wire a recovery token to a NONEXISTENT PI id. The handler's Stripe
    // GET will fail; we want to verify the token survives.
    let token = "phase7-transient-retry-token-ddd";
    let expires = OffsetDateTime::now_utc() + ::time::Duration::hours(1);
    seed_pending_row(
        &pool,
        &cust,
        TEST_WALLET_RESUME_TRANSIENT,
        &pm,
        "pi_does_not_exist_at_stripe_yyy",
        token,
        expires,
    )
    .await;

    let client = build_client(key.clone(), url.clone()).await;
    let resp = client
        .get(format!("/billing/auto_topup_resume?token={token}"))
        .dispatch()
        .await;
    // Pre-fix this assertion would say 404 because the lookup consumed
    // the token before the Stripe call. Post-fix we expect 503 (the
    // Stripe call failed) AND the token row is intact.
    assert_eq!(
        resp.status(),
        Status::ServiceUnavailable,
        "Stripe retrieve on bogus PI should yield 503"
    );

    let (token_after,): (Option<String>,) =
        sqlx::query_as("SELECT recovery_token FROM auto_topup_config WHERE customer_id = $1")
            .bind(&cust)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        token_after.as_deref(),
        Some(token),
        "transient Stripe failure must NOT burn the recovery token"
    );
}
