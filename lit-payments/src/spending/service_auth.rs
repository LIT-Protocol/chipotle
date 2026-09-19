//! `ServiceAuth` request guard — authenticates internal service-to-service
//! calls (the gateway reading rules / recording usage) via a shared bearer
//! token (`INTERNAL_SERVICE_TOKEN`).
//!
//! Distinct from the cookie-based [`Operator`](crate::auth::Operator) guard used
//! by the browser admin UI. If the token is not configured, internal endpoints
//! are disabled (503) rather than open.

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};

use crate::config::Config;

pub struct ServiceAuth;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ServiceAuth {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(cfg) = req.rocket().state::<Config>() else {
            tracing::error!("ServiceAuth guard: Config not in Rocket state");
            return Outcome::Error((Status::InternalServerError, ()));
        };
        let Some(expected) = cfg.internal_service_token.as_deref() else {
            // Not configured → internal endpoints are off, not open.
            return Outcome::Error((Status::ServiceUnavailable, ()));
        };
        match bearer(req) {
            Some(provided) if constant_time_eq(provided.as_bytes(), expected.as_bytes()) => {
                Outcome::Success(ServiceAuth)
            }
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

/// Extract a `Authorization: Bearer <token>` value.
fn bearer(req: &Request<'_>) -> Option<String> {
    let v = req.headers().get_one("Authorization")?;
    let mut parts = v.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(scheme), Some(token))
            if scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() =>
        {
            Some(token.trim().to_string())
        }
        _ => None,
    }
}

/// Length-aware constant-time byte comparison (the length is allowed to leak).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn equal_tokens_match() {
        assert!(constant_time_eq(b"s3cret-token", b"s3cret-token"));
    }

    #[test]
    fn different_tokens_or_lengths_fail() {
        assert!(!constant_time_eq(b"s3cret-token", b"s3cret-tokeX"));
        assert!(!constant_time_eq(b"short", b"longer-token"));
    }
}
