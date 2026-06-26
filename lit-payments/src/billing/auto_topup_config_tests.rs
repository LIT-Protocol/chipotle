//! Phase 4 integration tests for `GET` / `PUT /billing/auto_topup_config`.
//!
//! Requires `STRIPE_SECRET_KEY` (skip silently if absent) AND
//! `DATABASE_URL` pointed at the local dev Postgres with migrations
//! applied. The skip-on-missing-key pattern keeps the suite green on
//! machines without billing credentials configured.
//!
//! Each test uses a deterministic wallet address; the
//! [`super::setup_intent_tests::ensure_unique_customer`] helper handles
//! Stripe search-lag duplicate creation across reruns.
//!
//! Scenarios:
//!   - GET on a wallet with no DB row → 200 + `null`.
//!   - PUT enabled=false → 200, `disabled_reason = 'manual'`, GET roundtrip.
//!   - PUT enabled=true with all fields → 200, row persisted.
//!   - PUT enabled=true with `cap < topup_amount` → 400 (server validation).
//!   - PUT enabled=true with `topup < $5` → 400.
//!   - PUT enabled=true with null threshold → 400.
//!   - PUT with pm_xxx belonging to a DIFFERENT customer → 400
//!     (cross-tenant guard — codex gap #14).
//!   - Disable transition clears `pending_action_pi_id` + `recovery_token`
//!     (codex gap #15).
//!   - API-key caller → 501.

use std::sync::Arc;

use async_trait::async_trait;
use lit_billing_core::StripeClient;
use lit_billing_core::billing_auth::{
    AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload,
};
use rocket::http::{Header, Status};
use rocket::local::asynchronous::Client;
use rocket::{Rocket, routes};
use serde_json::Value;
use sqlx::PgPool;

use crate::auto_topup::types::AutoTopupConfigUpsert;
use crate::config::Config;

const TEST_WALLET_CRUD_BASIC: &str = "0xcccc0000cccc0000cccc0000cccc0000cccc0003";
const TEST_WALLET_CRUD_PM_OWNERSHIP: &str = "0xeeee0000eeee0000eeee0000eeee0000eeee0005";
const TEST_WALLET_CRUD_DISABLE: &str = "0xdddd0000dddd0000dddd0000dddd0000dddd0006";

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
        stripe_publishable_key: "pk_test_phase4_test".into(),
        max_grant_cents: 0,
        max_daily_per_operator_cents: 0,
        litkey_discount_basis_points: 0,
        litkey_chain: None,
        lit_api_server_base_url: "http://unused".into(),
        lit_internal_shared_secret: "unused".into(),
        lit_accounts_rpc_url: "http://localhost:8545".to_string(),
        lit_accounts_chain_id: 175188,
        lit_accounts_contract_address: alloy_primitives::Address::ZERO,
        stripe_webhook_secret: "unused".into(),
        reconciler_interval_secs: 900,
        enterprise_billing_interval_secs: 3600,
        stripe_dashboard_base: "https://dashboard.stripe.com".to_string(),
        cors_allowed_origins: vec!["http://localhost".to_string()],
    }
}

struct FixedWalletResolver {
    wallet: String,
}

#[async_trait]
impl AuthResolver for FixedWalletResolver {
    async fn verify_wallet_auth(
        &self,
        _payload: &WalletAuthPayload,
    ) -> Result<ResolvedIdentity, AuthError> {
        Ok(ResolvedIdentity {
            wallet_address_hex: self.wallet.clone(),
            api_key_hash_hex: format!("0x{}", "0".repeat(64)),
        })
    }
    async fn resolve_api_key(&self, _api_key: &str) -> Result<ResolvedIdentity, AuthError> {
        // Codex P1 (Phase 4): the handler now re-resolves API-key
        // callers through this same resolver to pull the wallet out.
        // Return Ok so the API-key test sees the normal flow (which
        // lands at 400 `no_stripe_customer` for a fresh wallet).
        Ok(ResolvedIdentity {
            wallet_address_hex: self.wallet.clone(),
            api_key_hash_hex: format!("0x{}", "0".repeat(64)),
        })
    }
}

async fn build_client(key: String, db_url: String, wallet: &str) -> Client {
    let cfg = test_config(key.clone(), db_url.clone());
    let stripe = StripeClient::new(key).expect("stripe client");
    let pool = PgPool::connect(&db_url).await.expect("connect db");
    let resolver: Arc<dyn AuthResolver> = Arc::new(FixedWalletResolver {
        wallet: wallet.to_string(),
    });
    let rocket = Rocket::build()
        .manage(cfg)
        .manage(stripe)
        .manage(pool)
        .manage(resolver)
        .mount(
            "/",
            routes![
                super::auto_topup_config::get_auto_topup_config,
                super::auto_topup_config::put_auto_topup_config,
            ],
        );
    Client::tracked(rocket).await.expect("rocket client")
}

fn wallet_header() -> String {
    let json = serde_json::json!({"typed_data":{"primaryType":"BillingAuth"},"signature":"0xdead"});
    base64_light::base64_encode(&serde_json::to_string(&json).unwrap())
}

use super::setup_intent_tests::attach_test_card;

/// Delete the `auto_topup_config` rows for this wallet so each test starts
/// from a clean slate. We delete by wallet_address (not customer_id) because
/// the schema enforces `UNIQUE(wallet_address)` — if test dedup churned the
/// Stripe customer id, the old row still holds the wallet lock and would
/// block a fresh UPSERT. Idempotent.
async fn reset_config_row_for_wallet(pool: &PgPool, wallet_address: &str) {
    let _ = sqlx::query("DELETE FROM auto_topup_config WHERE wallet_address = $1")
        .bind(wallet_address)
        .execute(pool)
        .await;
}

// ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial_test::serial]
async fn get_returns_null_when_no_row_exists() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_BASIC).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_BASIC).await;

    let client = build_client(key, url, TEST_WALLET_CRUD_BASIC).await;
    let resp = client
        .get("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let body: Value = resp.into_json().await.unwrap();
    assert!(body.is_null(), "expected null, got {body}");
}

#[tokio::test]
#[serial_test::serial]
async fn put_then_get_roundtrips_disabled_config() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_BASIC).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_BASIC).await;

    let client = build_client(key.clone(), url.clone(), TEST_WALLET_CRUD_BASIC).await;
    let body = AutoTopupConfigUpsert {
        enabled: false,
        threshold_cents: None,
        topup_amount_cents: None,
        monthly_cap_cents: None,
        payment_method_id: None,
        consent_version: None,
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let written: Value = resp.into_json().await.unwrap();
    assert_eq!(written["enabled"], false);
    assert_eq!(written["disabled_reason"], "manual");

    let resp = client
        .get("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let got: Value = resp.into_json().await.unwrap();
    assert_eq!(got["enabled"], false);
    assert_eq!(got["customer_id"], cust.as_str());
}

#[tokio::test]
#[serial_test::serial]
async fn put_enabled_with_all_fields_persists() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_BASIC).await;
    let pm_id = attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_BASIC).await;

    let client = build_client(key.clone(), url.clone(), TEST_WALLET_CRUD_BASIC).await;
    let body = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: Some(200),      // $2 threshold
        topup_amount_cents: Some(2_000), // $20 top-up
        monthly_cap_cents: Some(10_000), // $100/mo cap
        payment_method_id: Some(pm_id.clone()),
        consent_version: Some("v1".into()),
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(
        resp.status(),
        Status::Ok,
        "PUT failed: {}",
        resp.into_string().await.unwrap_or_default()
    );
    let got: Value = resp.into_json().await.unwrap();
    assert_eq!(got["enabled"], true);
    assert_eq!(got["threshold_cents"], 200);
    assert_eq!(got["topup_amount_cents"], 2_000);
    assert_eq!(got["monthly_cap_cents"], 10_000);
    assert_eq!(got["payment_method_id"], pm_id.as_str());
    assert!(got["disabled_reason"].is_null());
}

#[tokio::test]
#[serial_test::serial]
async fn put_enabled_with_cap_below_topup_rejected() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_BASIC).await;
    let pm_id = attach_test_card(&stripe, &cust).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_BASIC).await;

    let client = build_client(key.clone(), url.clone(), TEST_WALLET_CRUD_BASIC).await;
    let body = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: Some(200),
        topup_amount_cents: Some(2_000),
        monthly_cap_cents: Some(1_000), // < topup
        payment_method_id: Some(pm_id),
        consent_version: Some("v1".into()),
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.unwrap();
    assert_eq!(body["error"], "invalid_config");
}

#[tokio::test]
#[serial_test::serial]
async fn put_enabled_with_topup_below_floor_rejected() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url, TEST_WALLET_CRUD_BASIC).await;
    let body = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: Some(200),
        topup_amount_cents: Some(100), // $1 < $5 floor
        monthly_cap_cents: Some(1_000),
        payment_method_id: Some("pm_unused".into()),
        consent_version: Some("v1".into()),
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.unwrap();
    assert_eq!(body["error"], "invalid_config");
}

#[tokio::test]
#[serial_test::serial]
async fn put_enabled_with_null_threshold_rejected() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let client = build_client(key, url, TEST_WALLET_CRUD_BASIC).await;
    let body = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: None,
        topup_amount_cents: Some(2_000),
        monthly_cap_cents: Some(10_000),
        payment_method_id: Some("pm_unused".into()),
        consent_version: Some("v1".into()),
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.unwrap();
    assert_eq!(body["error"], "invalid_config");
}

#[tokio::test]
#[serial_test::serial]
async fn put_with_pm_owned_by_different_customer_rejected() {
    // Codex gap #14: cross-tenant pm_xxx. Caller A must not be able to
    // configure auto-topup against a card attached to caller B.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust_owner =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_PM_OWNERSHIP)
            .await;
    let other_cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_BASIC).await;
    // Attach the card to cust_owner.
    let pm_id = attach_test_card(&stripe, &cust_owner).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_BASIC).await;

    // Now wallet TEST_WALLET_CRUD_BASIC (other_cust) tries to claim that pm.
    let client = build_client(key.clone(), url.clone(), TEST_WALLET_CRUD_BASIC).await;
    let body = AutoTopupConfigUpsert {
        enabled: true,
        threshold_cents: Some(200),
        topup_amount_cents: Some(2_000),
        monthly_cap_cents: Some(10_000),
        payment_method_id: Some(pm_id),
        consent_version: Some("v1".into()),
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.unwrap();
    assert_eq!(body["error"], "payment_method_not_owned");
}

#[tokio::test]
#[serial_test::serial]
async fn put_disable_clears_pending_action_state() {
    // Codex gap #15: when a user opts out, pending SCA-recovery state must
    // be cleared so a re-enable can't re-trigger an old charge.
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    let cust =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_DISABLE).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_DISABLE).await;

    // Seed a row simulating "SCA pending": enabled=true with a saved card
    // PLUS pending_action_pi_id + recovery_token set. The webhook handler
    // would have written this in production (Phase 5).
    let pm_id = attach_test_card(&stripe, &cust).await;
    let now = time::OffsetDateTime::now_utc();
    sqlx::query(
        "INSERT INTO auto_topup_config (\
            customer_id, wallet_address, enabled, threshold_cents, \
            topup_amount_cents, monthly_cap_cents, payment_method_id, \
            consent_version, consent_signed_at, disabled_reason, \
            pending_action_pi_id, pending_action_at, recovery_token, \
            recovery_token_expires_at, updated_at) \
         VALUES ($1, $2, true, 200, 2000, 10000, $3, 'v1', $4, 'requires_action', \
            'pi_test_pending_xxx', $4, 'recovery_tok_xyz', $5, $4)",
    )
    .bind(&cust)
    .bind(TEST_WALLET_CRUD_DISABLE)
    .bind(&pm_id)
    .bind(now)
    .bind(now + time::Duration::hours(24))
    .execute(&pool)
    .await
    .expect("seed pending row");

    let client = build_client(key.clone(), url.clone(), TEST_WALLET_CRUD_DISABLE).await;
    let body = AutoTopupConfigUpsert {
        enabled: false,
        threshold_cents: None,
        topup_amount_cents: None,
        monthly_cap_cents: None,
        payment_method_id: None,
        consent_version: None,
    };
    let resp = client
        .put("/billing/auto_topup_config")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .json(&body)
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let got: Value = resp.into_json().await.unwrap();
    assert_eq!(got["enabled"], false);
    assert_eq!(got["disabled_reason"], "manual");
    assert!(got["pending_action_pi_id"].is_null());
    assert!(got["pending_action_at"].is_null());
    assert!(got["recovery_token"].is_null());
    assert!(got["recovery_token_expires_at"].is_null());
}

/// Codex P1 (Phase 4): the handler used to 501 API-key callers. Per
/// plan §5 these endpoints sit "behind the shared auth module" which
/// already verifies the key — a 501 contradicts the spec. The fix
/// re-resolves the API key through the same resolver the guard used,
/// derives the wallet, and continues the normal flow. With a fresh
/// (no-customer) wallet the flow lands at GET → 200 + null body just
/// like the wallet-sig path.
#[tokio::test]
#[serial_test::serial]
async fn api_key_caller_proceeds_to_normal_flow() {
    let Some(key) = stripe_key() else { return };
    let Some(url) = db_url() else { return };
    let stripe = StripeClient::new(key.clone()).unwrap();
    // Ensure the wallet has a Stripe customer so the GET path returns
    // 200 (Some / None depending on row presence), not 400. We use the
    // CRUD_BASIC wallet which `ensure_unique_customer` creates / finds.
    let _ =
        super::setup_intent_tests::ensure_unique_customer(&stripe, TEST_WALLET_CRUD_BASIC).await;
    let pool = PgPool::connect(&url).await.unwrap();
    reset_config_row_for_wallet(&pool, TEST_WALLET_CRUD_BASIC).await;

    let client = build_client(key, url, TEST_WALLET_CRUD_BASIC).await;
    let resp = client
        .get("/billing/auto_topup_config")
        .header(Header::new("X-Api-Key", "raw-api-key-abc"))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let body: Value = resp.into_json().await.unwrap();
    assert!(body.is_null(), "expected null (no row yet), got {body}");
}
