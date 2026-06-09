//! Internal Rocket routes mounted under `/internal/`.
//!
//! After glitch's #448 follow-up there is exactly one endpoint here:
//!
//!   `POST /internal/invalidate_balance_cache` — called by `lit-payments`
//!   after a successful sync credit so `lit-api-server`'s in-memory Stripe
//!   balance cache reflects the new balance immediately. Fire-and-forget on
//!   the lit-payments side; an unreached endpoint is a degraded path (cache
//!   self-heals via 10-minute TTL) but not a correctness problem.
//!
//! The previous `/internal/verify_wallet_auth` and `/internal/resolve_api_key`
//! endpoints were removed: the verifier moved into `lit-billing-core::eip712`
//! and the on-chain resolver into `lit-billing-core::on_chain`, so
//! lit-payments now performs both operations in-process via the same shared
//! primitives this service uses. Removing the endpoints eliminates the
//! `X-Internal-Secret` attack surface for auth, the cross-service availability
//! coupling, and ~80 LOC of HTTP plumbing that wrapped pure-function calls.

use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::Deserialize;
use rocket::serde::json::Json;
use rocket::{State, post};

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

#[cfg(test)]
mod tests {
    //! Phase 1 gate test: `/internal/invalidate_balance_cache` returns 200
    //! with the correct `X-Internal-Secret` header and 401 with a missing or
    //! mismatched one. The actual cache-invalidation side effect is covered
    //! by Stripe-side tests; here we only verify the auth shape, since that
    //! is the only thing Phase 1 ships.

    use std::sync::Arc;

    use rocket::http::{ContentType, Header, Status};
    use rocket::local::asynchronous::Client;
    use rocket::{Rocket, routes};

    use super::super::config::InternalConfig;
    use crate::stripe::StripeState;

    const SECRET: &str = "test-secret-very-long-value-12345678";

    async fn build() -> Client {
        let cfg = Some(Arc::new(InternalConfig {
            lit_internal_shared_secret: SECRET.to_string(),
        }));
        // The route signature takes `Option<Arc<StripeState>>`; ship `None`
        // — the handler short-circuits to 200 in that case (no cache wired)
        // and the auth guard runs first regardless, which is what we test.
        let stripe: Option<Arc<StripeState>> = None;
        let rocket = Rocket::build()
            .manage(cfg)
            .manage(stripe)
            .mount("/", routes![super::invalidate_balance_cache]);
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
}
