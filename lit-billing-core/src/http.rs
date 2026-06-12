//! Pure HTTP response parsing for the Stripe API.

use std::fmt;

use anyhow::Result;
use reqwest::StatusCode;

/// Parsed Stripe API response preserving the HTTP status code.
#[derive(Debug)]
pub struct StripeResponse {
    pub status: StatusCode,
    pub body: serde_json::Value,
}

/// Structured Stripe API error.
///
/// Keeps the full response body alongside the status code so callers can
/// reach for nested fields Stripe puts on `error`, e.g.
/// `error.payment_intent.id` — which the webhook handler needs to stage
/// the SCA recovery handoff (see lit-payments handler.rs).
///
/// Wrap in `anyhow::Error::new(...)` for propagation; downcast via
/// `e.downcast_ref::<StripeError>()` at the consumer side.
#[derive(Debug, Clone)]
pub struct StripeError {
    pub status: StatusCode,
    pub body: serde_json::Value,
}

impl StripeError {
    pub fn error_type(&self) -> Option<&str> {
        self.body.pointer("/error/type").and_then(|v| v.as_str())
    }
    pub fn code(&self) -> Option<&str> {
        self.body.pointer("/error/code").and_then(|v| v.as_str())
    }
    pub fn param(&self) -> Option<&str> {
        self.body.pointer("/error/param").and_then(|v| v.as_str())
    }
    pub fn message(&self) -> Option<&str> {
        self.body.pointer("/error/message").and_then(|v| v.as_str())
    }
    /// `error.payment_intent.id` — Stripe sets this on confirm-time
    /// failures (incl. `authentication_required`) so the SCA recovery
    /// flow can retrieve the same PI by id.
    pub fn payment_intent_id(&self) -> Option<&str> {
        self.body
            .pointer("/error/payment_intent/id")
            .and_then(|v| v.as_str())
    }
}

impl fmt::Display for StripeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Stripe error (HTTP {}, type={}, code={}, param={}, permission={}): {}",
            self.status,
            self.error_type().unwrap_or("unknown_type"),
            self.code().unwrap_or("unknown_code"),
            self.param().unwrap_or("unknown_param"),
            self.body
                .pointer("/error/permission")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_permission"),
            self.message().unwrap_or("unknown error"),
        )
    }
}

impl std::error::Error for StripeError {}

/// Parse a Stripe API response from raw status + body text.
///
/// Accepts `(StatusCode, &str)` rather than `reqwest::Response` so this logic
/// is trivially unit-testable without mocking HTTP.
///
/// On Stripe-side errors (response body contains `"error": {...}`), this
/// returns `Err(anyhow::Error::new(StripeError { ... }))`. Callers that
/// need the structured error (e.g. to extract `error.payment_intent.id`)
/// downcast via `err.downcast_ref::<StripeError>()`. The `Display` impl
/// preserves the previous "HTTP X, type=, code=, ..." format so existing
/// log lines and tests that assert on the message stay valid.
pub fn parse_stripe_response(status: StatusCode, body_text: &str) -> Result<StripeResponse> {
    let body: serde_json::Value = serde_json::from_str(body_text)
        .map_err(|e| anyhow::anyhow!("Stripe: invalid JSON (HTTP {status}): {e}"))?;

    if body.get("error").is_some() {
        return Err(anyhow::Error::new(StripeError { status, body }));
    }

    Ok(StripeResponse { status, body })
}

pub(crate) fn stripe_base() -> &'static str {
    "https://api.stripe.com/v1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stripe_response_2xx_success() {
        let body = r#"{"id": "cus_123", "object": "customer"}"#;
        let resp = parse_stripe_response(StatusCode::OK, body).unwrap();
        assert_eq!(resp.status, StatusCode::OK);
        assert_eq!(resp.body["id"], "cus_123");
    }

    #[test]
    fn parse_stripe_response_4xx_with_error() {
        let body = r#"{"error": {"message": "Invalid API Key provided", "type": "authentication_error", "code": "api_key_invalid", "param": "api_key", "permission": "customers_read"}}"#;
        let err = parse_stripe_response(StatusCode::UNAUTHORIZED, body).unwrap_err();
        // Backwards-compat: the Display string still carries the legacy
        // diagnostic fields so existing logs and tests keep working.
        let msg = err.to_string();
        assert!(msg.contains("HTTP 401"), "expected HTTP 401 in: {msg}");
        assert!(
            msg.contains("type=authentication_error"),
            "expected error type in: {msg}"
        );
        assert!(
            msg.contains("code=api_key_invalid"),
            "expected error code in: {msg}"
        );
        assert!(
            msg.contains("param=api_key"),
            "expected error param in: {msg}"
        );
        assert!(
            msg.contains("permission=customers_read"),
            "expected missing permission in: {msg}"
        );
        assert!(
            msg.contains("Invalid API Key provided"),
            "expected error message in: {msg}"
        );
        // New: structured access through downcast.
        let se = err
            .downcast_ref::<StripeError>()
            .expect("error is StripeError");
        assert_eq!(se.error_type(), Some("authentication_error"));
        assert_eq!(se.code(), Some("api_key_invalid"));
    }

    #[test]
    fn parse_stripe_response_5xx_with_error() {
        let body = r#"{"error": {"message": "Internal server error", "type": "api_error"}}"#;
        let err = parse_stripe_response(StatusCode::INTERNAL_SERVER_ERROR, body).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("HTTP 500"), "expected HTTP 500 in: {msg}");
    }

    #[test]
    fn parse_stripe_response_error_without_message() {
        let body = r#"{"error": {"type": "api_error"}}"#;
        let err = parse_stripe_response(StatusCode::BAD_REQUEST, body).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown error"),
            "expected 'unknown error' in: {msg}"
        );
    }

    #[test]
    fn parse_stripe_response_non_json() {
        let body = "<html>Bad Gateway</html>";
        let err = parse_stripe_response(StatusCode::BAD_GATEWAY, body).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid JSON"),
            "expected 'invalid JSON' in: {msg}"
        );
        assert!(msg.contains("HTTP 502"), "expected HTTP 502 in: {msg}");
    }

    #[test]
    fn parse_stripe_response_2xx_with_no_error_field() {
        let body = r#"{"balance": -500, "currency": "usd"}"#;
        let resp = parse_stripe_response(StatusCode::OK, body).unwrap();
        assert_eq!(resp.body["balance"], -500);
    }

    #[test]
    fn stripe_error_surfaces_payment_intent_id() {
        // Stripe's `authentication_required` shape: error.payment_intent.id
        // carries the PI to retry. Codex P1 #1 — handler must read this
        // structured field instead of regexing the formatted message.
        let body = r#"{
            "error": {
                "type": "card_error",
                "code": "authentication_required",
                "message": "Your card was declined. This transaction requires authentication.",
                "payment_intent": {
                    "id": "pi_3NopeYrGhjEGDNSRy42abc",
                    "status": "requires_action"
                }
            }
        }"#;
        let err = parse_stripe_response(StatusCode::PAYMENT_REQUIRED, body).unwrap_err();
        let se = err
            .downcast_ref::<StripeError>()
            .expect("error is StripeError");
        assert_eq!(se.code(), Some("authentication_required"));
        assert_eq!(se.payment_intent_id(), Some("pi_3NopeYrGhjEGDNSRy42abc"));
    }
}
