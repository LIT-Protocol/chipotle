//! Authenticated HTTP client for the Stripe REST API.

use std::time::Duration;

use anyhow::Result;

use crate::http::{StripeResponse, parse_stripe_response, stripe_base};

/// Stripe API version pinned for every request this client makes.
///
/// `2020-08-27` is the floor required for the Customer Search API
/// (used by `customer::find_by_wallet`). Stripe accounts that haven't been
/// upgraded inherit the version they were created on — pinning here means
/// we get consistent behaviour regardless of the per-account default.
///
/// Bump deliberately: any change to Stripe response shapes between versions
/// will surface as deserialisation errors in tests against the newer API
/// before it ever reaches production.
const STRIPE_API_VERSION: &str = "2020-08-27";

/// Stripe API client: secret key + reqwest client.
///
/// No caching, no in-process state beyond credentials. Build once and clone
/// freely — `reqwest::Client` is cheap to clone (Arc internally).
#[derive(Clone)]
pub struct StripeClient {
    // NOTE: Debug is implemented manually below to redact secret_key.
    secret_key: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for StripeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeClient")
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

impl StripeClient {
    /// Build a new client with the given Stripe secret key.
    pub fn new(secret_key: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("stripe: failed to build HTTP client: {e}"))?;
        Ok(Self { secret_key, client })
    }

    /// `GET /v1/<path>` with optional query params.
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<StripeResponse> {
        let url = format!("{}/{}", stripe_base(), path);
        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.secret_key, Some(""))
            .header("Stripe-Version", STRIPE_API_VERSION)
            .query(query)
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        parse_stripe_response(status, &body_text)
    }

    /// `POST /v1/<path>` with form-encoded body.
    pub async fn post(&self, path: &str, params: &[(&str, &str)]) -> Result<StripeResponse> {
        let url = format!("{}/{}", stripe_base(), path);
        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.secret_key, Some(""))
            .header("Stripe-Version", STRIPE_API_VERSION)
            .form(params)
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        parse_stripe_response(status, &body_text)
    }

    /// `POST /v1/<path>` with form-encoded body and an `Idempotency-Key` header.
    ///
    /// Stripe deduplicates requests sharing the same key within 24 hours, making
    /// retries after network errors safe from producing duplicate side-effects.
    pub async fn post_with_idempotency(
        &self,
        path: &str,
        params: &[(&str, &str)],
        idempotency_key: &str,
    ) -> Result<StripeResponse> {
        let url = format!("{}/{}", stripe_base(), path);
        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.secret_key, Some(""))
            .header("Stripe-Version", STRIPE_API_VERSION)
            .header("Idempotency-Key", idempotency_key)
            .form(params)
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        parse_stripe_response(status, &body_text)
    }
}
