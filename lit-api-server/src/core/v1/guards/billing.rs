/// Rocket request guards that extract the API key and enforce billing.
///
/// - `BilledManagementApiKey`: verifies the account can cover the flat $0.01
///   management charge, then marks the request for settlement. The actual
///   charge happens in [`ManagementBillingFairing`] only after the handler
///   produced a success response — so a request that fails validation,
///   authorization, or execution is never charged.
/// - `BilledLitActionApiKey`: checks credit availability only; per-second
///   billing happens during execution via `UpdateResourceUsage`.
///
/// If Stripe is not configured (no `StripeState` managed), guards succeed
/// without charging.
///
/// Failure mapping (previously everything was a 402):
/// - key missing or not resolving to an account → `401 Unauthorized`
/// - account exists but cannot cover the operation → `402 Payment Required`
/// - chain RPC / Stripe outage → `503 Service Unavailable`
///
/// Each failure also attaches an [`ErrorDetail`](crate::core::v1::catchers::ErrorDetail)
/// so the JSON catchers explain what happened and how to fix it.
use std::sync::Arc;

use rocket::Response;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

use crate::core::v1::catchers::{DASHBOARD_URL, set_error_detail};
use crate::stripe::{self, BillingError, StripeState, cents_to_display};
use tracing::instrument;

// ─── Helpers ──────────────────────────────────────────────────────────────────

#[instrument(name = "billing::extract_api_key", skip_all)]
fn extract_api_key(request: &Request<'_>) -> Option<String> {
    // Authorization: Bearer <key>
    if let Some(v) = request.headers().get_one("Authorization") {
        let mut parts = v.split_whitespace();
        if let (Some(scheme), Some(key)) = (parts.next(), parts.next())
            && scheme.eq_ignore_ascii_case("bearer")
            && !key.trim().is_empty()
        {
            return Some(key.trim().to_string());
        }
    }
    // X-Api-Key: <key>
    if let Some(key) = request.headers().get_one("X-Api-Key") {
        let key = key.trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    None
}

/// Fail with 401 + a detail explaining how to authenticate.
fn missing_key_outcome<T>(request: &Request<'_>) -> Outcome<T, ()> {
    set_error_detail(
        request,
        "No API key provided.",
        "Send your key in the X-Api-Key header (or Authorization: Bearer <key>). \
         Create an account with POST /core/v1/new_account.",
    );
    Outcome::Error((Status::Unauthorized, ()))
}

/// Reject an API-key header whose shape is a precomputed 32-byte account hash
/// (`0x<64hex>`). Such identities must arrive only through the verified
/// `X-Wallet-Auth` (EIP-712) path: `usage_api_key_to_hash` passes this shape
/// through as an already-hashed identity (CPL-285), so a caller who sends
/// `X-Api-Key: 0x{keccak256(walletAddress)}` would resolve straight to a
/// victim's on-chain billing account without proving ownership. `BillingAuth`
/// in lit-billing-core already rejects this on its legacy API-key path; the
/// two Billed* guards did not. Not currently exploitable end-to-end (downstream
/// re-hashing → contract revert → no charge), but this closes the gap at the
/// request boundary as defense in depth (CPL-379 L1).
fn reject_precomputed_hash_shape<T>(request: &Request<'_>) -> Outcome<T, ()> {
    tracing::warn!(
        "rejecting API-key header that looks like a precomputed account hash; \
         ChainSecured callers must use X-Wallet-Auth"
    );
    set_error_detail(
        request,
        "API key looks like a precomputed account hash, not a raw key.",
        "ChainSecured callers must authenticate with X-Wallet-Auth (EIP-712 signature), \
         not by sending the account hash as an API key.",
    );
    Outcome::Error((Status::Unauthorized, ()))
}

/// Map a [`BillingError`] to an HTTP status and attach a specific detail for
/// the JSON catchers.
fn billing_error_status(request: &Request<'_>, e: BillingError) -> Status {
    match e {
        BillingError::InvalidApiKey => {
            set_error_detail(
                request,
                "API key not recognized — it does not resolve to any account.",
                "Double-check the key (it is shown only once at creation) and verify it with \
                 GET /core/v1/account_exists. Or create a new account with \
                 POST /core/v1/new_account.",
            );
            Status::Unauthorized
        }
        BillingError::InsufficientCredits {
            available_cents,
            required_cents,
        } => {
            set_error_detail(
                request,
                format!(
                    "Insufficient credits: this call needs {} but your balance is {}.",
                    cents_to_display(required_cents),
                    cents_to_display(available_cents),
                ),
                if available_cents == 0 {
                    // A $0 balance can also mean the customer was funded moments
                    // ago and Stripe's search index hasn't caught up yet (#555)
                    // — that case is transient and heals within about a minute.
                    format!(
                        "Add funds (minimum $5.00, card or crypto) in the dashboard at \
                         {DASHBOARD_URL} or via POST /core/v1/billing/create_payment_intent. \
                         If you added funds within the last minute, retry shortly — new \
                         billing accounts can take up to a minute to become visible. \
                         Check your balance with GET /core/v1/billing/balance."
                    )
                } else {
                    format!(
                        "Add funds (minimum $5.00, card or crypto) in the dashboard at \
                         {DASHBOARD_URL} or via POST /core/v1/billing/create_payment_intent. \
                         Check your balance with GET /core/v1/billing/balance."
                    )
                },
            );
            Status::PaymentRequired
        }
        BillingError::Unavailable(err) => {
            tracing::warn!("billing guard: upstream unavailable: {err}");
            set_error_detail(
                request,
                "Billing is temporarily unavailable — your request was not charged.",
                "Retry in a few seconds.",
            );
            Status::ServiceUnavailable
        }
    }
}

// ─── BilledManagementApiKey ───────────────────────────────────────────────────

/// Guards a management endpoint ($0.01 per call, settled after success).
pub struct BilledManagementApiKey(pub String);

/// Request-local marker: the guard verified credit and the response fairing
/// should settle the management charge if (and only if) the handler succeeds.
#[derive(Default)]
struct PendingManagementCharge(Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BilledManagementApiKey {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(key) = extract_api_key(request) else {
            return missing_key_outcome(request);
        };
        if crate::utils::parse_with_hash::is_precomputed_hash_shape(&key) {
            return reject_precomputed_hash_shape(request);
        }

        if let Some(state) = request.rocket().state::<Option<Arc<StripeState>>>()
            && let Some(stripe) = state.as_ref()
        {
            match stripe::check_credit(
                &key,
                stripe::COST_MANAGEMENT_CENTS,
                stripe::BillingReason::Management,
                stripe,
            )
            .await
            {
                Ok(()) => {
                    request.local_cache(|| PendingManagementCharge(Some(key.clone())));
                }
                Err(e) => {
                    return Outcome::Error((billing_error_status(request, e), ()));
                }
            }
        }

        Outcome::Success(BilledManagementApiKey(key))
    }
}

/// Settles the $0.01 management charge after the handler succeeded.
///
/// Charging here instead of in the guard means a request that fails body
/// parsing (400/422), authorization (403), or execution (5xx) costs nothing.
/// The window between the guard's credit check and settlement lets concurrent
/// requests slightly overdraw — the same staleness already accepted for the
/// balance cache (CPL-246).
pub struct ManagementBillingFairing;

#[rocket::async_trait]
impl Fairing for ManagementBillingFairing {
    fn info(&self) -> Info {
        Info {
            name: "Management billing settlement",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        if response.status().class() != rocket::http::StatusClass::Success {
            return;
        }
        let pending = request.local_cache(PendingManagementCharge::default);
        let Some(key) = pending.0.as_ref() else {
            return;
        };
        let Some(state) = request.rocket().state::<Option<Arc<StripeState>>>() else {
            return;
        };
        let Some(stripe) = state.as_ref() else {
            return;
        };
        // resolve/customer/balance are warm in cache from the guard, so this is
        // a fast local decrement plus a spawned (fire-and-forget) Stripe POST.
        if let Err(e) = stripe::charge_management(key, stripe).await {
            // The operation already completed; never fail the response over
            // settlement. Surfaced via the billing settlement metrics/logs.
            tracing::error!("management charge settlement failed (operation completed): {e}");
        }
    }
}

impl<'r> OpenApiFromRequest<'r> for BilledManagementApiKey {
    fn from_request_input(
        generator: &mut OpenApiGenerator,
        _name: String,
        required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        let schema = generator.json_schema::<String>();
        Ok(RequestHeaderInput::Parameter(Parameter {
            name: "X-Api-Key".to_owned(),
            location: "header".to_owned(),
            description: Some(
                "Account or usage API key. Alternatively use Authorization: Bearer <key>."
                    .to_owned(),
            ),
            required,
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

// ─── BilledLitActionApiKey ────────────────────────────────────────────────────

/// Guards a lit-action endpoint.
///
/// Validates that the API key has credits available (Stripe balance < 0)
/// but does NOT charge upfront — per-second billing happens during execution
/// via the `UpdateResourceUsage` opcode.
pub struct BilledLitActionApiKey(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BilledLitActionApiKey {
    type Error = ();

    #[instrument(name = "billing::BilledLitActionApiKey::from_request", skip_all)]
    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(key) = extract_api_key(request) else {
            return missing_key_outcome(request);
        };
        if crate::utils::parse_with_hash::is_precomputed_hash_shape(&key) {
            return reject_precomputed_hash_shape(request);
        }

        // If Stripe is configured, verify the customer has credits available.
        // Requiring 1 cent reproduces the old `balance >= 0 → reject` rule.
        if let Some(state) = request.rocket().state::<Option<Arc<StripeState>>>()
            && let Some(stripe) = state.as_ref()
            && let Err(e) =
                stripe::check_credit(&key, 1, stripe::BillingReason::LitAction, stripe).await
        {
            return Outcome::Error((billing_error_status(request, e), ()));
        }

        Outcome::Success(BilledLitActionApiKey(key))
    }
}

impl<'r> OpenApiFromRequest<'r> for BilledLitActionApiKey {
    #[instrument(name = "billing::BilledLitActionApiKey::from_request_input", skip_all)]
    fn from_request_input(
        generator: &mut OpenApiGenerator,
        _name: String,
        required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        let schema = generator.json_schema::<String>();
        Ok(RequestHeaderInput::Parameter(Parameter {
            name: "X-Api-Key".to_owned(),
            location: "header".to_owned(),
            description: Some(
                "Account or usage API key. Alternatively use Authorization: Bearer <key>."
                    .to_owned(),
            ),
            required,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Header;
    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};

    /// A lowercase 0x-prefixed 32-byte hex string — the precomputed-hash shape
    /// that must never be accepted as a raw API key (CPL-379 L1 / CPL-285).
    const HASH_KEY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

    #[get("/lit-action")]
    fn lit_action_probe(key: BilledLitActionApiKey) -> String {
        key.0
    }

    #[get("/management")]
    fn management_probe(key: BilledManagementApiKey) -> String {
        key.0
    }

    /// With no `StripeState` managed, both guards short-circuit the billing
    /// check and succeed for a normal key — so these tests isolate the
    /// precomputed-hash rejection from any Stripe interaction.
    async fn client() -> Client {
        let rocket = rocket::build().mount("/", routes![lit_action_probe, management_probe]);
        Client::tracked(rocket).await.expect("valid rocket")
    }

    #[tokio::test]
    async fn lit_action_guard_rejects_precomputed_hash_in_x_api_key() {
        let c = client().await;
        let resp = c
            .get("/lit-action")
            .header(Header::new("X-Api-Key", HASH_KEY))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn lit_action_guard_rejects_precomputed_hash_in_bearer() {
        let c = client().await;
        let resp = c
            .get("/lit-action")
            .header(Header::new("Authorization", format!("Bearer {HASH_KEY}")))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn management_guard_rejects_precomputed_hash() {
        let c = client().await;
        let resp = c
            .get("/management")
            .header(Header::new("X-Api-Key", HASH_KEY))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[tokio::test]
    async fn guards_accept_a_normal_raw_key() {
        // A raw key is not hash-shaped; with Stripe unconfigured the guard
        // admits it (no charge) and the probe echoes it back unchanged.
        let c = client().await;
        for path in ["/lit-action", "/management"] {
            let resp = c
                .get(path)
                .header(Header::new("X-Api-Key", "a-normal-raw-api-key"))
                .dispatch()
                .await;
            assert_eq!(resp.status(), Status::Ok, "path {path}");
            assert_eq!(
                resp.into_string().await.unwrap_or_default(),
                "a-normal-raw-api-key"
            );
        }
    }
}
