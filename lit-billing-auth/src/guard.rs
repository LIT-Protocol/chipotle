//! Rocket request guard yielding [`BillingAuth`] — service-agnostic.
//!
//! The guard pulls an `Arc<dyn AuthResolver>` from Rocket state and dispatches
//! signature / key verification through it. Each service supplies its own
//! resolver, so this guard runs unchanged on both lit-api-server (local
//! in-process resolver) and lit-payments (HTTP resolver pointing at
//! lit-api-server).

use std::sync::Arc;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};

use crate::is_precomputed_hash_shape;
use crate::resolver::{AuthError, AuthResolver, WalletAuthPayload};

/// Identity proven by an inbound billing request. Same shape as the original
/// lit-api-server `BillingAuth` enum so handlers migrate by switching the
/// import path only.
#[derive(Clone, Debug)]
pub enum BillingAuth {
    /// Raw API key (master or usage). Downstream code hashes it via
    /// `usage_api_key_to_hash` to derive the on-chain account.
    ApiKey(String),
    /// Verified wallet signature. Carries the wallet-derived
    /// `keccak256(walletAddress)` hex hash for direct use as the identity
    /// string passed to `resolve_wallet_address` (no further hashing needed).
    WalletSigned {
        wallet_address_hex: String,
        api_key_hash_hex: String,
    },
}

impl BillingAuth {
    /// String to pass to `stripe::resolve_wallet_address`. For API-key flows
    /// this is the raw key (gets hashed downstream). For wallet-signed flows
    /// this is the precomputed `0x{keccak256_hex(walletAddress)}`.
    pub fn identity_string(&self) -> &str {
        match self {
            BillingAuth::ApiKey(k) => k.as_str(),
            BillingAuth::WalletSigned {
                api_key_hash_hex, ..
            } => api_key_hash_hex.as_str(),
        }
    }
}

fn map_auth_error(e: AuthError) -> Status {
    match e {
        AuthError::BadCredentials(_) | AuthError::Forbidden(_) => Status::Unauthorized,
        AuthError::Transient(_) => Status::ServiceUnavailable,
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BillingAuth {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<BillingAuth, Self::Error> {
        // DEV-ONLY bypass — compiled out of release binaries entirely.
        //
        // `LIT_DEV_WALLET_BYPASS=1` + `X-Dev-Wallet: 0x...` short-circuits
        // EIP-712 verification for local browser QA when lit-api-server
        // (which owns the on-chain resolver) isn't running. Glitch's PR
        // review correctly flagged that a runtime env switch in a Stripe
        // billing guard is a security-audit liability even if currently
        // unset in prod. Gating behind `cfg(debug_assertions)` means the
        // production binary literally cannot include this code path —
        // `cargo build --release` strips it and the env var becomes inert.
        #[cfg(debug_assertions)]
        if std::env::var("LIT_DEV_WALLET_BYPASS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            && let Some(dev_wallet) = request.headers().get_one("X-Dev-Wallet")
            && dev_wallet.starts_with("0x")
            && dev_wallet.len() == 42
        {
            tracing::warn!(
                "LIT_DEV_WALLET_BYPASS active — accepting X-Dev-Wallet={dev_wallet}. \
                 DEBUG BUILD ONLY."
            );
            let wallet_lower = dev_wallet.to_ascii_lowercase();
            let api_key_hash_hex = format!(
                "0x{}",
                wallet_lower[2..]
                    .repeat(2)
                    .chars()
                    .take(64)
                    .collect::<String>()
            );
            return Outcome::Success(BillingAuth::WalletSigned {
                wallet_address_hex: wallet_lower,
                api_key_hash_hex,
            });
        }

        // Resolver is required — without it no auth can be done. A misconfigured
        // service should fail closed (500), never silently allow.
        let resolver = match request.rocket().state::<Arc<dyn AuthResolver>>() {
            Some(r) => r.clone(),
            None => {
                tracing::error!(
                    "BillingAuth guard: Arc<dyn AuthResolver> missing from Rocket state"
                );
                return Outcome::Error((Status::InternalServerError, ()));
            }
        };

        // Wallet-signed path takes precedence ONLY when the header parses cleanly.
        // A malformed X-Wallet-Auth (bad base64, bad JSON) falls through to the
        // API-key path so a junk-header proxy or stale localStorage entry doesn't
        // lock out an otherwise-valid API key. A well-formed but signature-invalid
        // payload IS a 401 — the caller explicitly claimed wallet identity and we
        // reject it.
        if let Some(encoded) = request.headers().get_one("X-Wallet-Auth") {
            let bytes = base64_light::base64_decode(encoded.trim());
            if !bytes.is_empty()
                && let Ok(payload) = serde_json::from_slice::<WalletAuthPayload>(&bytes)
            {
                match resolver.verify_wallet_auth(&payload).await {
                    Ok(identity) => {
                        return Outcome::Success(BillingAuth::WalletSigned {
                            wallet_address_hex: identity.wallet_address_hex,
                            api_key_hash_hex: identity.api_key_hash_hex,
                        });
                    }
                    Err(e) => return Outcome::Error((map_auth_error(e), ())),
                }
            }
            tracing::warn!(
                "X-Wallet-Auth header present but unparseable; falling through to API key"
            );
        }

        // Legacy API-key path. Reject any string shaped like a precomputed
        // account hash here — those must come through the verified WalletSigned
        // path only. Otherwise an attacker could send
        // `X-Api-Key: 0x{keccak256(walletAddress)}` and bypass the EIP-712 path
        // entirely (CPL-285 / CPL-286).
        //
        // Codex P1 (Phase 2) fix: the guard must VERIFY the key, not just
        // accept its presence. We delegate to `resolver.resolve_api_key`
        // which (on lit-api-server) hits the on-chain
        // `allApiKeyHashesToMaster` mapping and (on lit-payments) forwards
        // via the internal HTTP hop. A non-existent / bogus key yields
        // BadCredentials → 401; transient resolver failures yield
        // Transient → 503 so retries are signalled correctly.
        let api_key = extract_api_key(request);
        if let Some(key) = api_key {
            if is_precomputed_hash_shape(&key) {
                tracing::warn!(
                    "rejecting API-key header that looks like a precomputed account hash; \
                     ChainSecured callers must use X-Wallet-Auth"
                );
                return Outcome::Error((Status::Unauthorized, ()));
            }
            match resolver.resolve_api_key(&key).await {
                Ok(_identity) => {
                    // Yield the raw key so downstream callers can keep
                    // their existing keccak256/cache-key derivations
                    // unchanged. The resolver call was the verification;
                    // we don't carry its result forward to avoid changing
                    // the public surface of `BillingAuth::ApiKey`.
                    return Outcome::Success(BillingAuth::ApiKey(key));
                }
                Err(e) => return Outcome::Error((map_auth_error(e), ())),
            }
        }
        Outcome::Error((Status::Unauthorized, ()))
    }
}

/// Pull a non-empty API key out of either `Authorization: Bearer <k>` or
/// `X-Api-Key: <k>`. Returns `None` if neither header is present or
/// usable. Kept separate from the verification path so test code can
/// exercise extraction without spinning up a resolver.
fn extract_api_key(request: &Request<'_>) -> Option<String> {
    if let Some(v) = request.headers().get_one("Authorization") {
        let v = v.trim();
        let mut parts = v.split_whitespace();
        if let (Some(scheme), Some(key_part)) = (parts.next(), parts.next())
            && scheme.eq_ignore_ascii_case("bearer")
        {
            let key = key_part.trim();
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
    }
    if let Some(key) = request.headers().get_one("X-Api-Key") {
        let key = key.trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    None
}

#[cfg(feature = "openapi")]
mod openapi_impl {
    use super::BillingAuth;
    use rocket_okapi::Result as RocketOkapiResult;
    use rocket_okapi::r#gen::OpenApiGenerator;
    use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
    use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

    impl<'r> OpenApiFromRequest<'r> for BillingAuth {
        fn from_request_input(
            generator: &mut OpenApiGenerator,
            _name: String,
            _required: bool,
        ) -> RocketOkapiResult<RequestHeaderInput> {
            let schema = generator.json_schema::<String>();
            Ok(RequestHeaderInput::Parameter(Parameter {
                name: "X-Api-Key".to_owned(),
                location: "header".to_owned(),
                description: Some(
                    "API-mode auth: account or usage API key (alternatively \
                     `Authorization: Bearer <key>`). OR — for ChainSecured \
                     callers — omit X-Api-Key entirely and send \
                     `X-Wallet-Auth: <base64(JSON{typed_data, signature})>` \
                     where `typed_data` is EIP-712 with \
                     `primaryType: \"BillingAuth\"`. The signature proves \
                     wallet possession; the typed data must include the \
                     connected wallet address and an issuedAt timestamp \
                     within ±5 minutes."
                        .to_owned(),
                ),
                required: false,
                deprecated: false,
                allow_empty_value: false,
                value: ParameterValue::Schema {
                    style: None,
                    explode: None,
                    allow_reserved: false,
                    schema,
                    example: None,
                    examples: None,
                },
                extensions: Object::default(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Phase 2 gate tests for the `BillingAuth` Rocket guard, exercised
    //! against a mock `AuthResolver`. These cover the full decision tree:
    //!
    //!   • valid wallet signature → 200 + WalletSigned identity
    //!   • tampered signature (resolver BadCredentials) → 401
    //!   • resolver Transient → 503 (don't surface as auth failure)
    //!   • malformed X-Wallet-Auth → fall through to API-key path
    //!   • valid Authorization: Bearer → 200 + ApiKey identity
    //!   • valid X-Api-Key → 200 + ApiKey identity
    //!   • precomputed-hash shape in X-Api-Key → 401 (CPL-285 hardening)
    //!   • precomputed-hash shape in Authorization → 401 (CPL-285 hardening)
    //!   • empty X-Api-Key → 401
    //!   • no auth headers → 401
    //!   • missing AuthResolver from state → 500 (fail closed)
    //!
    //! Mocking the resolver isolates the guard from on-chain / EIP-712
    //! details; those are covered by lit-api-server's own EIP-712 tests
    //! and by the anvil-backed integration test in Phase 2.6.

    use super::*;
    use crate::resolver::{AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload};
    use async_trait::async_trait;
    use rocket::http::{Header, Status};
    use rocket::local::asynchronous::Client;
    use rocket::{Rocket, get, routes};
    use std::sync::Arc;

    const TEST_WALLET: &str = "0x1111111111111111111111111111111111111111";
    const TEST_HASH: &str = "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    /// Mock resolver that returns whatever the test sets up. Each variant
    /// corresponds to one branch we want to exercise on the guard.
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
            match self.mode {
                MockMode::Success => Ok(ResolvedIdentity {
                    wallet_address_hex: TEST_WALLET.to_string(),
                    api_key_hash_hex: TEST_HASH.to_string(),
                }),
                MockMode::BadCredentials => Err(AuthError::BadCredentials("mock".into())),
                MockMode::Transient => Err(AuthError::Transient("mock".into())),
            }
        }

        async fn resolve_api_key(&self, _api_key: &str) -> Result<ResolvedIdentity, AuthError> {
            // API-key flow isn't routed through the resolver by the guard
            // itself (the guard yields BillingAuth::ApiKey with the raw key
            // and downstream code calls the resolver). Return success in
            // case future code changes start calling it.
            Ok(ResolvedIdentity {
                wallet_address_hex: TEST_WALLET.to_string(),
                api_key_hash_hex: TEST_HASH.to_string(),
            })
        }
    }

    /// Test-only handler that yields the resolved identity string so we
    /// can assert which branch the guard took.
    #[get("/probe")]
    fn probe(auth: BillingAuth) -> String {
        auth.identity_string().to_string()
    }

    async fn build(mode: MockMode) -> Client {
        let resolver: Arc<dyn AuthResolver> = Arc::new(MockResolver { mode });
        let rocket = Rocket::build().manage(resolver).mount("/", routes![probe]);
        Client::tracked(rocket).await.expect("rocket client")
    }

    /// `X-Wallet-Auth` carries a base64-encoded JSON payload. The mock
    /// resolver doesn't care about the contents — just that the payload
    /// decodes — so any non-empty base64-JSON works.
    fn wallet_header_value() -> String {
        let payload = serde_json::json!({
            "typed_data": { "primaryType": "BillingAuth" },
            "signature": "0xdeadbeef",
        });
        let json = serde_json::to_string(&payload).unwrap();
        base64_light::base64_encode(&json)
    }

    #[tokio::test]
    async fn wallet_sig_valid_returns_wallet_signed_identity() {
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Wallet-Auth", wallet_header_value()))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        // identity_string() on WalletSigned returns the api_key_hash_hex.
        assert_eq!(resp.into_string().await.unwrap_or_default(), TEST_HASH);
    }

    #[tokio::test]
    async fn wallet_sig_bad_credentials_returns_401() {
        let client = build(MockMode::BadCredentials).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Wallet-Auth", wallet_header_value()))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn wallet_sig_transient_returns_503() {
        let client = build(MockMode::Transient).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Wallet-Auth", wallet_header_value()))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::ServiceUnavailable);
    }

    #[tokio::test]
    async fn malformed_wallet_header_falls_through_to_api_key() {
        // base64-decoding "not-base64-!!" yields empty bytes; the guard
        // logs a warning and falls through to the API-key path so a junk
        // header from a misbehaving proxy / stale browser cache doesn't
        // lock out an otherwise-valid API key caller.
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Wallet-Auth", "!!!not_base64!!!"))
            .header(Header::new("X-Api-Key", "valid-api-key"))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        assert_eq!(
            resp.into_string().await.unwrap_or_default(),
            "valid-api-key"
        );
    }

    #[tokio::test]
    async fn bearer_api_key_returns_api_key_identity() {
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("Authorization", "Bearer my-secret-key"))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        assert_eq!(
            resp.into_string().await.unwrap_or_default(),
            "my-secret-key"
        );
    }

    #[tokio::test]
    async fn x_api_key_returns_api_key_identity() {
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Api-Key", "my-secret-key"))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        assert_eq!(
            resp.into_string().await.unwrap_or_default(),
            "my-secret-key"
        );
    }

    /// CPL-285: a precomputed account hash sent in `X-Api-Key` would have
    /// bypassed the EIP-712 wallet-signed path entirely. The guard must
    /// reject this shape outright in the API-key branch.
    #[tokio::test]
    async fn precomputed_hash_in_x_api_key_is_rejected() {
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Api-Key", TEST_HASH))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn precomputed_hash_in_bearer_is_rejected() {
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("Authorization", format!("Bearer {TEST_HASH}")))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn empty_x_api_key_returns_401() {
        let client = build(MockMode::Success).await;
        let resp = client
            .get("/probe")
            .header(Header::new("X-Api-Key", ""))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn no_auth_headers_returns_401() {
        let client = build(MockMode::Success).await;
        let resp = client.get("/probe").dispatch().await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn missing_resolver_returns_500() {
        // Build a Rocket without registering the resolver. The guard must
        // fail closed rather than silently allow.
        let rocket = Rocket::build().mount("/", routes![probe]);
        let client = Client::tracked(rocket).await.expect("rocket client");
        let resp = client
            .get("/probe")
            .header(Header::new("X-Wallet-Auth", wallet_header_value()))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::InternalServerError);
    }

    #[test]
    fn identity_string_returns_raw_key_for_apikey_variant() {
        let auth = BillingAuth::ApiKey("my-raw-key".to_string());
        assert_eq!(auth.identity_string(), "my-raw-key");
    }

    #[test]
    fn identity_string_returns_hash_for_walletsigned_variant() {
        let auth = BillingAuth::WalletSigned {
            wallet_address_hex: TEST_WALLET.to_string(),
            api_key_hash_hex: TEST_HASH.to_string(),
        };
        assert_eq!(auth.identity_string(), TEST_HASH);
    }
}
