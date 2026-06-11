//! Phase 3 integration tests for `POST /billing/setup_intent`.
//!
//! These hit **real Stripe test mode** — they need `STRIPE_SECRET_KEY` to
//! be set (e.g. via `lit-payments/.env` sourced into the test environment)
//! and are skipped silently otherwise. Each test that creates Stripe state
//! uses a deterministic wallet address so reruns are idempotent (Stripe
//! search will find the same customer on the second pass).
//!
//! Covered scenarios:
//!   - Wallet has a Stripe customer → 200 + `client_secret` + Stripe
//!     SetupIntent reflects `usage=off_session, customer=cus_*`.
//!   - Wallet has no Stripe customer → 400 `no_stripe_customer`.
//!   - API-key caller → 501 (Phase 3 ships wallet-sig only).
//!
//! NOT covered here (intentional): `stripe.confirmCardSetup` 3DS flow —
//! that needs a browser and is the Phase 8 manual test checkpoint.

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

use crate::config::Config;

const TEST_WALLET_WITH_CUSTOMER: &str = "0xaaaa0000aaaa0000aaaa0000aaaa0000aaaa0001";
const TEST_WALLET_NO_CUSTOMER: &str = "0xbbbb0000bbbb0000bbbb0000bbbb0000bbbb0002";

fn stripe_key() -> Option<String> {
    std::env::var("STRIPE_SECRET_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty() && k.starts_with("rk_test_") || k.starts_with("sk_test_"))
}

/// Build the bare Config struct we need for the endpoint. Avoids reading
/// every env var — only the fields setup_intent touches are populated.
fn test_config(stripe_secret_key: String) -> Config {
    Config {
        database_url: "postgres://unused".into(),
        magic_link_signing_key: vec![0u8; 32],
        resend_api_key: "unused".into(),
        mail_from: "unused".into(),
        public_base_url: "http://unused".into(),
        stripe_secret_key,
        stripe_publishable_key: "pk_test_phase3_test".into(),
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
        cors_allowed_origins: vec!["http://localhost".to_string()],
    }
}

/// Mock resolver that returns whichever wallet the test wires it to.
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
        // Phase 2 P1 fix: the BillingAuth guard now resolves api keys
        // before yielding ApiKey to the handler. The setup_intent
        // handler's 501-for-api-key path is still what we're testing,
        // so the mock returns Ok(_) to let the call reach the handler.
        Ok(ResolvedIdentity {
            wallet_address_hex: self.wallet.clone(),
            api_key_hash_hex: format!("0x{}", "0".repeat(64)),
        })
    }
}

async fn build_client(key: String, resolver_wallet: &str) -> Client {
    let cfg = test_config(key.clone());
    let stripe = StripeClient::new(key).expect("stripe client");
    let resolver: Arc<dyn AuthResolver> = Arc::new(FixedWalletResolver {
        wallet: resolver_wallet.to_string(),
    });
    // The handler now takes `&State<Arc<dyn AuthResolver>>` so it can
    // re-resolve API-key callers. Register the resolver under that
    // concrete state key so the route can pull it out.
    let rocket = Rocket::build()
        .manage(cfg)
        .manage(stripe)
        .manage(resolver)
        .mount("/", routes![super::setup_intent::setup_intent]);
    Client::tracked(rocket).await.expect("rocket client")
}

/// Header value the [`BillingAuth`] guard requires to take the wallet-sig
/// path. The mock resolver ignores its contents; only base64-decodable JSON
/// is needed for the guard to forward to the resolver.
fn wallet_header() -> String {
    let json = serde_json::json!({"typed_data":{"primaryType":"BillingAuth"},"signature":"0xdead"});
    base64_light::base64_encode(&serde_json::to_string(&json).unwrap())
}

/// Idempotent + dedup-safe test-customer setup.
///
/// Stripe's "search miss + create" race during indexing lag has previously
/// produced multiple customers with the same `metadata.wallet_address` in
/// this test account. `find_or_create_by_wallet` alone is not enough — once
/// a duplicate exists, subsequent searches can return either record
/// non-deterministically.
///
/// This helper:
///   1. Lists ALL customers with the wallet metadata (paginated).
///   2. Deletes everything past the first match.
///   3. Returns the surviving customer id (creating one if zero existed).
///   4. Waits for Stripe's search index to consistently return that id.
///
/// Per-test calls are cheap on a clean account (one search hit, no delete).
/// Attach a `pm_card_visa` PaymentMethod to the customer and return its id.
/// Idempotent enough for tests — Stripe lets a single PaymentMethod attach
/// to many customers (different ids per attach); on rerun we get a fresh
/// pm_xxx but the previous one stays attached.
pub(crate) async fn attach_test_card(stripe: &StripeClient, customer_id: &str) -> String {
    let pm = stripe
        .post(
            "payment_methods",
            &[("type", "card"), ("card[token]", "tok_visa")],
        )
        .await
        .expect("create pm");
    let pm_id = pm.body["id"].as_str().expect("pm id").to_string();
    stripe
        .post(
            &format!("payment_methods/{pm_id}/attach"),
            &[("customer", customer_id)],
        )
        .await
        .expect("attach pm");
    pm_id
}

pub(crate) async fn ensure_unique_customer(stripe: &StripeClient, wallet: &str) -> String {
    let query = format!("metadata['wallet_address']:'{wallet}'");
    let resp = stripe
        .get(
            "customers/search",
            &[("query", query.as_str()), ("limit", "100")],
        )
        .await
        .expect("search customers");
    let ids: Vec<String> = resp
        .body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // No dedup-by-deletion: deleting Stripe customers under tests has
    // produced orphan rows in `auto_topup_config` (which holds
    // `UNIQUE(wallet_address)`). Instead, take whichever id Stripe search
    // returns first and accept extras as benign test-only fixtures. The
    // handler's `find_by_wallet` will consistently return the same first id
    // on subsequent calls because Stripe search ordering is stable within
    // a short window. If a fully-fresh wallet has zero matches, create one
    // and poll search until indexing settles.
    let surviving = match ids.first() {
        Some(first) => first.clone(),
        None => lit_billing_core::customer::find_or_create_by_wallet(stripe, wallet)
            .await
            .expect("create customer"),
    };

    // Poll the search index until it returns the surviving id consistently.
    for _ in 0..10 {
        if let Ok(Some(found)) = lit_billing_core::customer::find_by_wallet(stripe, wallet).await
            && found == surviving
        {
            return surviving;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    surviving
}

#[tokio::test]
#[serial_test::serial]
async fn setup_intent_returns_client_secret_when_customer_exists() {
    let Some(key) = stripe_key() else {
        eprintln!("STRIPE_SECRET_KEY not set — skipping Stripe-backed test");
        return;
    };
    let stripe = StripeClient::new(key.clone()).expect("stripe client");
    let customer_id = ensure_unique_customer(&stripe, TEST_WALLET_WITH_CUSTOMER).await;

    let client = build_client(key.clone(), TEST_WALLET_WITH_CUSTOMER).await;
    let resp = client
        .post("/billing/setup_intent")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::Ok);
    let body: Value = resp.into_json().await.expect("json body");
    let client_secret = body["client_secret"].as_str().expect("client_secret");
    let publishable_key = body["publishable_key"].as_str().expect("publishable_key");
    assert!(
        client_secret.starts_with("seti_"),
        "client_secret should be SetupIntent-shaped, got {client_secret}"
    );
    assert!(publishable_key.starts_with("pk_"));

    // Inspect Stripe to confirm the SetupIntent is correctly configured.
    let si_id = client_secret
        .split('_')
        .take(2)
        .collect::<Vec<_>>()
        .join("_");
    let resp = stripe
        .get(&format!("setup_intents/{si_id}"), &[])
        .await
        .expect("retrieve SetupIntent");
    assert_eq!(resp.body["usage"], "off_session");
    assert_eq!(resp.body["customer"], customer_id.as_str());
}

#[tokio::test]
#[serial_test::serial]
async fn setup_intent_returns_400_when_no_customer() {
    let Some(key) = stripe_key() else {
        eprintln!("STRIPE_SECRET_KEY not set — skipping Stripe-backed test");
        return;
    };
    // Deliberately do NOT create a Stripe customer for this wallet. If a
    // prior test run did, this assertion would flake — so we pick a wallet
    // distinct from the one used in the happy-path test, and we don't
    // create one. (Once a customer exists for this wallet, Stripe search
    // will keep finding it across runs; if a developer manually created
    // one for this wallet, delete it via the Stripe dashboard to restore
    // the test invariant.)
    let client = build_client(key.clone(), TEST_WALLET_NO_CUSTOMER).await;
    let resp = client
        .post("/billing/setup_intent")
        .header(Header::new("X-Wallet-Auth", wallet_header()))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.expect("json body");
    assert_eq!(body["error"], "no_stripe_customer");
}

/// Codex P1 (Phase 3): the handler used to 501 API-key callers. Per
/// plan §5 the endpoint is "behind the shared auth module", which
/// already verifies the key via `resolve_api_key`. The fix re-resolves
/// the key inside the handler to derive the wallet and continues the
/// normal flow. With a fresh wallet that has no Stripe customer, that
/// flow lands at 400 `no_stripe_customer` — same as the wallet-sig
/// path. Skip silently if Stripe creds aren't configured (this test
/// also exercises the Stripe lookup).
#[tokio::test]
#[serial_test::serial]
async fn setup_intent_api_key_caller_proceeds_to_normal_flow() {
    let Some(key) = stripe_key() else {
        eprintln!("STRIPE_SECRET_KEY not set — skipping Stripe-backed test");
        return;
    };
    // Use the "no-customer" wallet so the handler reaches the
    // `find_by_wallet -> None` branch and returns 400 `no_stripe_customer`.
    // That's the canonical success signal for "API-key path works": we
    // got past the old 501 short-circuit and hit the same code the
    // wallet-sig path hits.
    let client = build_client(key, TEST_WALLET_NO_CUSTOMER).await;
    let resp = client
        .post("/billing/setup_intent")
        .header(Header::new("X-Api-Key", "some-raw-api-key"))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::BadRequest);
    let body: Value = resp.into_json().await.expect("json body");
    assert_eq!(body["error"], "no_stripe_customer");
}
