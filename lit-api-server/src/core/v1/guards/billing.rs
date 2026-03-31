/// Rocket request guards that extract the API key and enforce billing.
///
/// - `BilledManagementApiKey`: charges upfront ($0.01 per call).
/// - `BilledLitActionApiKey`: checks credit availability only; per-second billing
///   happens during execution via `UpdateResourceUsage`.
///
/// If Stripe is not configured (no `StripeState` managed), guards succeed without charging.
/// If the customer has insufficient credits the guard fails with `402 Payment Required`.
use std::sync::Arc;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

use crate::stripe::{self, StripeState};

// ─── Helper ───────────────────────────────────────────────────────────────────

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

async fn charge_guard(
    request: &Request<'_>,
    charge_fn: impl AsyncFn(&str, &StripeState) -> anyhow::Result<()>,
) -> Outcome<String, ()> {
    let Some(key) = extract_api_key(request) else {
        return Outcome::Error((Status::Unauthorized, ()));
    };

    if let Some(state) = request.rocket().state::<Option<Arc<StripeState>>>()
        && let Some(stripe) = state.as_ref()
        && let Err(e) = charge_fn(&key, stripe).await
    {
        tracing::warn!("billing guard charge failed: {e}");
        return Outcome::Error((Status::PaymentRequired, ()));
    }

    Outcome::Success(key)
}

// ─── BilledManagementApiKey ───────────────────────────────────────────────────

/// Guards a management endpoint ($0.01 per call).
pub struct BilledManagementApiKey(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BilledManagementApiKey {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        charge_guard(request, stripe::charge_management)
            .await
            .map(BilledManagementApiKey)
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

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(key) = extract_api_key(request) else {
            return Outcome::Error((Status::Unauthorized, ()));
        };

        // If Stripe is configured, verify the customer has credits available.
        if let Some(state) = request.rocket().state::<Option<Arc<StripeState>>>()
            && let Some(stripe) = state.as_ref()
        {
            match stripe::resolve_wallet_address(&key, stripe).await {
                Ok(wallet) => {
                    match stripe::get_customer_by_wallet(&wallet, stripe).await {
                        Ok(customer_id) => {
                            match stripe::get_credit_balance(&customer_id, stripe).await {
                                Ok(balance) if balance >= 0 => {
                                    tracing::warn!(
                                        "billing guard: insufficient credits (balance={balance})"
                                    );
                                    return Outcome::Error((Status::PaymentRequired, ()));
                                }
                                Err(e) => {
                                    tracing::warn!("billing guard: balance check failed: {e}");
                                    return Outcome::Error((Status::PaymentRequired, ()));
                                }
                                _ => {} // balance < 0 means credits available
                            }
                        }
                        Err(e) => {
                            tracing::warn!("billing guard: customer lookup failed: {e}");
                            return Outcome::Error((Status::PaymentRequired, ()));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("billing guard: wallet resolution failed: {e}");
                    return Outcome::Error((Status::PaymentRequired, ()));
                }
            }
        }

        Outcome::Success(BilledLitActionApiKey(key))
    }
}

impl<'r> OpenApiFromRequest<'r> for BilledLitActionApiKey {
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
