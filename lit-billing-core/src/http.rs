//! Pure HTTP response parsing for the Stripe API.

use anyhow::Result;
use reqwest::StatusCode;

/// Parsed Stripe API response preserving the HTTP status code.
#[derive(Debug)]
pub struct StripeResponse {
    pub status: StatusCode,
    pub body: serde_json::Value,
}

/// Parse a Stripe API response from raw status + body text.
///
/// Accepts `(StatusCode, &str)` rather than `reqwest::Response` so this logic
/// is trivially unit-testable without mocking HTTP.
pub fn parse_stripe_response(status: StatusCode, body_text: &str) -> Result<StripeResponse> {
    let body: serde_json::Value = serde_json::from_str(body_text)
        .map_err(|e| anyhow::anyhow!("Stripe: invalid JSON (HTTP {status}): {e}"))?;

    if let Some(e) = body.get("error") {
        let msg = e
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("Stripe error (HTTP {status}): {msg}");
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
        let body =
            r#"{"error": {"message": "Invalid API Key provided", "type": "authentication_error"}}"#;
        let err = parse_stripe_response(StatusCode::UNAUTHORIZED, body).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("HTTP 401"), "expected HTTP 401 in: {msg}");
        assert!(
            msg.contains("Invalid API Key provided"),
            "expected error message in: {msg}"
        );
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
}
