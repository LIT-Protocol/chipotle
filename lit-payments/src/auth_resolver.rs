//! HTTP-based [`AuthResolver`] — delegates verification to lit-api-server.
//!
//! `lit-payments` has no on-chain access (no signer pool, no AccountConfig
//! bindings) and we intentionally keep the EIP-712 verifier in exactly one
//! place. This resolver forwards verification to lit-api-server's
//! `POST /internal/verify_wallet_auth` + `POST /internal/resolve_api_key`
//! endpoints over the existing `X-Internal-Secret` channel.
//!
//! Latency: roughly one TCP round-trip + a short EIP-712 verify or on-chain
//! lookup on the api-server side. Dashboard requests already take tens of
//! milliseconds end-to-end, so this hop is negligible. The api-server caches
//! API-key → wallet resolutions for 1 hour, so repeated API-key auth on the
//! same key after the first call is sub-millisecond on the api-server side.

use async_trait::async_trait;
use lit_billing_core::billing_auth::{AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize)]
struct VerifyWalletAuthRequest<'a> {
    typed_data: &'a serde_json::Value,
    signature: &'a str,
}

#[derive(Serialize)]
struct ResolveApiKeyRequest<'a> {
    api_key: &'a str,
}

#[derive(Deserialize)]
struct ResolverResponse {
    wallet_address_hex: String,
    api_key_hash_hex: String,
}

pub struct HttpAuthResolver {
    client: Client,
    base_url: String,
    secret: String,
}

impl HttpAuthResolver {
    /// Build a resolver pointing at `lit-api-server`. `base_url` should be
    /// the same `LIT_API_SERVER_BASE_URL` used by the cache-invalidation
    /// callback. `secret` is the shared `LIT_INTERNAL_SHARED_SECRET`.
    pub fn new(base_url: String, secret: String) -> Result<Self, anyhow::Error> {
        let client = Client::builder().timeout(TIMEOUT).build()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            secret,
        })
    }

    async fn post<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<ResolverResponse, AuthError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("X-Internal-Secret", &self.secret)
            .json(body)
            .send()
            .await
            .map_err(|e| AuthError::Transient(format!("POST {path}: {e}")))?;

        match resp.status() {
            StatusCode::OK => resp
                .json::<ResolverResponse>()
                .await
                .map_err(|e| AuthError::Transient(format!("decode {path}: {e}"))),
            StatusCode::UNAUTHORIZED => {
                Err(AuthError::BadCredentials(format!("{path} returned 401")))
            }
            // 503 specifically is "transient backend"; map back to the same.
            StatusCode::SERVICE_UNAVAILABLE => {
                Err(AuthError::Transient(format!("{path} returned 503")))
            }
            other => Err(AuthError::Transient(format!(
                "{path} unexpected status {other}"
            ))),
        }
    }
}

#[async_trait]
impl AuthResolver for HttpAuthResolver {
    async fn verify_wallet_auth(
        &self,
        payload: &WalletAuthPayload,
    ) -> Result<ResolvedIdentity, AuthError> {
        let body = VerifyWalletAuthRequest {
            typed_data: &payload.typed_data,
            signature: &payload.signature,
        };
        let r = self.post("/internal/verify_wallet_auth", &body).await?;
        Ok(ResolvedIdentity {
            wallet_address_hex: r.wallet_address_hex,
            api_key_hash_hex: r.api_key_hash_hex,
        })
    }

    async fn resolve_api_key(&self, api_key: &str) -> Result<ResolvedIdentity, AuthError> {
        let body = ResolveApiKeyRequest { api_key };
        let r = self.post("/internal/resolve_api_key", &body).await?;
        Ok(ResolvedIdentity {
            wallet_address_hex: r.wallet_address_hex,
            api_key_hash_hex: r.api_key_hash_hex,
        })
    }
}
