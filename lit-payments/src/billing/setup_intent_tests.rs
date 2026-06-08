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
use lit_billing_auth::{AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload};
use lit_billing_core::StripeClient;
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
        stripe_webhook_secret: "unused".into(),
        reconciler_interval_secs: 900,
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
        // The endpoint short-circuits to 501 before calling this for
        // ApiKey-shaped BillingAuth, so this branch is unreachable in tests.
        Err(AuthError::BadCredentials("unused".into()))
    }
}

async fn build_client(key: String, resolver_wallet: &str) -> Client {
    let cfg = test_config(key.clone());
    let stripe = StripeClient::new(key).expect("stripe client");
    let resolver: Arc<dyn AuthResolver> = Arc::new(FixedWalletResolver {
        wallet: resolver_wallet.to_string(),
    });
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
async fn ensure_unique_customer(stripe: &StripeClient, wallet: &str) -> String {
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

    let surviving = match ids.split_first() {
        Some((first, rest)) => {
            for dupe in rest {
                tracing::warn!("deleting duplicate customer {dupe} for wallet {wallet}");
                let _ = stripe.post(&format!("customers/{dupe}"), &[]).await;
                // Stripe customer DELETE requires the customers/{id} DELETE
                // HTTP verb, not POST. Use a raw reqwest call so we don't
                // bloat the shared client surface.
                let _ = reqwest::Client::new()
                    .delete(format!("https://api.stripe.com/v1/customers/{dupe}"))
                    .basic_auth(
                        std::env::var("STRIPE_SECRET_KEY").unwrap_or_default(),
                        Some(""),
                    )
                    .header("Stripe-Version", "2020-08-27")
                    .send()
                    .await;
            }
            first.clone()
        }
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

#[tokio::test]
async fn setup_intent_returns_501_for_api_key_caller() {
    // Stripe key NOT required for this path — the handler short-circuits
    // before calling Stripe. Skip only if the build can't even construct
    // a Rocket (it always can, since we pass a dummy key).
    let client = build_client(
        "rk_test_unused_for_this_test_xxx".to_string(),
        TEST_WALLET_NO_CUSTOMER,
    )
    .await;
    let resp = client
        .post("/billing/setup_intent")
        .header(Header::new("X-Api-Key", "some-raw-api-key"))
        .dispatch()
        .await;
    assert_eq!(resp.status(), Status::NotImplemented);
    let body: Value = resp.into_json().await.expect("json body");
    assert_eq!(body["error"], "api_key_setup_intent_unsupported");
}
