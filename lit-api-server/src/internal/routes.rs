//! Internal Rocket routes mounted under `/internal/`.
//!
//! Only one endpoint in the new design:
//!   `POST /internal/invalidate_balance_cache` — called by `lit-payments`
//!   after a successful sync credit so `lit-api-server`'s in-memory Stripe
//!   balance cache reflects the new balance immediately. Fire-and-forget on
//!   the lit-payments side; an unreached endpoint is a degraded path (cache
//!   self-heals via 10-minute TTL) but not a correctness problem.

use std::sync::Arc;

use lit_billing_core::billing_auth::{
    AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload,
};
use rocket::http::Status;
use rocket::serde::Deserialize;
use rocket::serde::json::Json;
use rocket::{State, post};
use serde::Serialize;

use super::guard::InternalSecret;
use crate::stripe::StripeState;

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct InvalidateBalanceCacheRequest {
    pub customer_id: String,
}

#[post("/internal/invalidate_balance_cache", format = "json", data = "<body>")]
pub async fn invalidate_balance_cache(
    _auth: InternalSecret,
    body: Json<InvalidateBalanceCacheRequest>,
    stripe_state: &State<Option<Arc<StripeState>>>,
) -> Status {
    let Some(state) = stripe_state.inner() else {
        // Stripe wiring disabled in this build — nothing to invalidate.
        // Still return 200 so the caller doesn't log a spurious failure;
        // there's no cache to be wrong about.
        return Status::Ok;
    };
    state.invalidate_balance_cache(&body.customer_id).await;
    Status::Ok
}

/// Auth-verification endpoints used by `lit-payments`'s `HttpAuthResolver`.
///
/// `lit-payments` does not have on-chain access (no signer pool, no chain
/// bindings) and we deliberately keep the EIP-712 verifier in a single
/// place. These two endpoints expose the in-process resolver over the
/// existing `X-Internal-Secret` channel so lit-payments can authenticate
/// dashboard requests by delegating here.

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct VerifyWalletAuthRequest {
    pub typed_data: serde_json::Value,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyWalletAuthResponse {
    pub wallet_address_hex: String,
    pub api_key_hash_hex: String,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct ResolveApiKeyRequest {
    pub api_key: String,
}

fn map_resolver_err(e: AuthError) -> Status {
    match e {
        AuthError::BadCredentials(_) | AuthError::Forbidden(_) => Status::Unauthorized,
        AuthError::Transient(_) => Status::ServiceUnavailable,
    }
}

fn to_response(identity: ResolvedIdentity) -> Json<VerifyWalletAuthResponse> {
    Json(VerifyWalletAuthResponse {
        wallet_address_hex: identity.wallet_address_hex,
        api_key_hash_hex: identity.api_key_hash_hex,
    })
}

#[post("/internal/verify_wallet_auth", format = "json", data = "<body>")]
pub async fn verify_wallet_auth(
    _auth: InternalSecret,
    body: Json<VerifyWalletAuthRequest>,
    resolver: &State<Arc<dyn AuthResolver>>,
) -> Result<Json<VerifyWalletAuthResponse>, Status> {
    let payload = WalletAuthPayload {
        typed_data: body.typed_data.clone(),
        signature: body.signature.clone(),
    };
    resolver
        .verify_wallet_auth(&payload)
        .await
        .map(to_response)
        .map_err(map_resolver_err)
}

#[post("/internal/resolve_api_key", format = "json", data = "<body>")]
pub async fn resolve_api_key(
    _auth: InternalSecret,
    body: Json<ResolveApiKeyRequest>,
    resolver: &State<Arc<dyn AuthResolver>>,
) -> Result<Json<VerifyWalletAuthResponse>, Status> {
    resolver
        .resolve_api_key(&body.api_key)
        .await
        .map(to_response)
        .map_err(map_resolver_err)
}

#[cfg(test)]
mod tests {
    //! Phase 1 gate test: `/internal/invalidate_balance_cache` returns 200
    //! with the correct `X-Internal-Secret` header and 401 with a missing or
    //! mismatched one. The actual cache-invalidation side effect is covered
    //! by Stripe-side tests; here we only verify the auth shape, since that
    //! is the only thing Phase 1 ships.

    use std::sync::Arc;

    use async_trait::async_trait;
    use lit_billing_core::billing_auth::{
    AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload,
};
    use rocket::http::{ContentType, Header, Status};
    use rocket::local::asynchronous::Client;
    use rocket::{Rocket, routes};

    use super::super::config::InternalConfig;
    use crate::stripe::StripeState;

    const SECRET: &str = "test-secret-very-long-value-12345678";
    const TEST_WALLET: &str = "0x1111111111111111111111111111111111111111";
    const TEST_HASH: &str = "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    /// Mock resolver used to exercise the verify_wallet_auth / resolve_api_key
    /// endpoints without touching the real EIP-712 verifier or on-chain
    /// resolver. Each call site picks the mode it wants.
    enum MockMode {
        Success,
        BadCredentials,
        Transient,
    }

    struct MockResolver {
        mode: MockMode,
    }

    #[async_trait]
    impl AuthResolver for MockResolver {
        async fn verify_wallet_auth(
            &self,
            _payload: &WalletAuthPayload,
        ) -> Result<ResolvedIdentity, AuthError> {
            self.respond()
        }
        async fn resolve_api_key(&self, _api_key: &str) -> Result<ResolvedIdentity, AuthError> {
            self.respond()
        }
    }

    impl MockResolver {
        fn respond(&self) -> Result<ResolvedIdentity, AuthError> {
            match self.mode {
                MockMode::Success => Ok(ResolvedIdentity {
                    wallet_address_hex: TEST_WALLET.to_string(),
                    api_key_hash_hex: TEST_HASH.to_string(),
                }),
                MockMode::BadCredentials => Err(AuthError::BadCredentials("mock".into())),
                MockMode::Transient => Err(AuthError::Transient("mock".into())),
            }
        }
    }

    async fn build() -> Client {
        build_with_resolver(MockMode::Success).await
    }

    async fn build_with_resolver(mode: MockMode) -> Client {
        let cfg = Some(Arc::new(InternalConfig {
            lit_internal_shared_secret: SECRET.to_string(),
        }));
        // The route signature takes `Option<Arc<StripeState>>`; ship `None`
        // — the handler short-circuits to 200 in that case (no cache wired)
        // and the auth guard runs first regardless, which is what we test.
        let stripe: Option<Arc<StripeState>> = None;
        let resolver: Arc<dyn AuthResolver> = Arc::new(MockResolver { mode });
        let rocket = Rocket::build()
            .manage(cfg)
            .manage(stripe)
            .manage(resolver)
            .mount(
                "/",
                routes![
                    super::invalidate_balance_cache,
                    super::verify_wallet_auth,
                    super::resolve_api_key,
                ],
            );
        Client::tracked(rocket).await.expect("rocket client")
    }

    #[tokio::test]
    async fn rejects_missing_header() {
        let client = build().await;
        let resp = client
            .post("/internal/invalidate_balance_cache")
            .header(ContentType::JSON)
            .body(r#"{"customer_id":"cus_test"}"#)
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn rejects_wrong_secret() {
        let client = build().await;
        let resp = client
            .post("/internal/invalidate_balance_cache")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", "definitely-wrong"))
            .body(r#"{"customer_id":"cus_test"}"#)
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn accepts_correct_secret() {
        let client = build().await;
        let resp = client
            .post("/internal/invalidate_balance_cache")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", SECRET))
            .body(r#"{"customer_id":"cus_test"}"#)
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
    }

    #[tokio::test]
    async fn rejects_secret_of_different_length() {
        // Length-mismatch path: the guard short-circuits before ct_eq.
        // Verify the same 401 outcome.
        let client = build().await;
        let resp = client
            .post("/internal/invalidate_balance_cache")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", "short"))
            .body(r#"{"customer_id":"cus_test"}"#)
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    // ─── verify_wallet_auth ─────────────────────────────────────────────────

    fn wallet_body() -> String {
        // The body shape mirrors what lit-payments's HttpAuthResolver
        // serialises from a `WalletAuthPayload`. The mock resolver ignores
        // its contents — we only test the route plumbing here.
        r#"{"typed_data":{"primaryType":"BillingAuth"},"signature":"0xdead"}"#.to_string()
    }

    #[tokio::test]
    async fn verify_wallet_auth_requires_secret() {
        let client = build_with_resolver(MockMode::Success).await;
        let resp = client
            .post("/internal/verify_wallet_auth")
            .header(ContentType::JSON)
            .body(wallet_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn verify_wallet_auth_returns_identity_on_success() {
        let client = build_with_resolver(MockMode::Success).await;
        let resp = client
            .post("/internal/verify_wallet_auth")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", SECRET))
            .body(wallet_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        let body = resp.into_string().await.unwrap_or_default();
        assert!(body.contains(TEST_WALLET));
        assert!(body.contains(TEST_HASH));
    }

    #[tokio::test]
    async fn verify_wallet_auth_returns_401_on_bad_credentials() {
        let client = build_with_resolver(MockMode::BadCredentials).await;
        let resp = client
            .post("/internal/verify_wallet_auth")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", SECRET))
            .body(wallet_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn verify_wallet_auth_returns_503_on_transient() {
        let client = build_with_resolver(MockMode::Transient).await;
        let resp = client
            .post("/internal/verify_wallet_auth")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", SECRET))
            .body(wallet_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::ServiceUnavailable);
    }

    // ─── resolve_api_key ────────────────────────────────────────────────────

    fn api_key_body() -> String {
        r#"{"api_key":"raw-key-abc-123"}"#.to_string()
    }

    #[tokio::test]
    async fn resolve_api_key_requires_secret() {
        let client = build_with_resolver(MockMode::Success).await;
        let resp = client
            .post("/internal/resolve_api_key")
            .header(ContentType::JSON)
            .body(api_key_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn resolve_api_key_returns_identity_on_success() {
        let client = build_with_resolver(MockMode::Success).await;
        let resp = client
            .post("/internal/resolve_api_key")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", SECRET))
            .body(api_key_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        let body = resp.into_string().await.unwrap_or_default();
        assert!(body.contains(TEST_WALLET));
    }

    #[tokio::test]
    async fn resolve_api_key_returns_503_on_transient() {
        // The on-chain resolver can fail transiently (RPC down). The route
        // must propagate that as a retriable 503, not a 401 — otherwise
        // callers will start showing users a "your key is bad" message for
        // every RPC blip.
        let client = build_with_resolver(MockMode::Transient).await;
        let resp = client
            .post("/internal/resolve_api_key")
            .header(ContentType::JSON)
            .header(Header::new("X-Internal-Secret", SECRET))
            .body(api_key_body())
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::ServiceUnavailable);
    }
}
