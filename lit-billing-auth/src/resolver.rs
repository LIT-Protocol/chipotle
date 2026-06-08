//! `AuthResolver` trait — service-agnostic identity verification.
//!
//! Each service plugs in its own implementation:
//!
//! - **lit-api-server** uses a local resolver that calls the existing
//!   in-process EIP-712 verifier and the on-chain `allApiKeyHashesToMaster`
//!   resolver. Auth happens fully inside the TEE.
//!
//! - **lit-payments** uses an HTTP resolver that forwards the verification
//!   to lit-api-server's internal endpoints. Keeps the on-chain plumbing in
//!   exactly one place (the TEE) and saves lit-payments from having to
//!   re-implement the same logic.
//!
//! The [`BillingAuth`] Rocket guard pulls an `Arc<dyn AuthResolver>` from
//! Rocket state and dispatches verification to whichever impl the service
//! installed.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Failure modes from a resolver call. Each case maps to a Rocket
/// `Status` by the [`crate::guard::BillingAuth`] `FromRequest` impl —
/// `BadCredentials` / `Forbidden` → 401, `Transient` → 503.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Header shaped correctly but signature / key did not verify.
    #[error("auth credentials rejected: {0}")]
    BadCredentials(String),
    /// Auth verified, but identity is not authorised for billing (e.g.
    /// a precomputed-hash key sent in the API-key header).
    #[error("identity forbidden: {0}")]
    Forbidden(String),
    /// Transient backend failure (resolver could not reach Stripe / on-chain
    /// RPC / lit-api-server). Caller should retry; not a 401.
    #[error("auth resolver transient failure: {0}")]
    Transient(String),
}

/// Payload carried inside the base64-decoded `X-Wallet-Auth` header.
///
/// `typed_data` is the full EIP-712 typed-data the wallet signed; resolvers
/// recover the signer from it and validate the schema. We avoid making this
/// crate depend on `alloy` by treating the typed data as opaque JSON; the
/// resolver implementation owns the parsing.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletAuthPayload {
    pub typed_data: serde_json::Value,
    pub signature: String,
}

/// Identity returned by a successful verification — used by the Rocket guard
/// to populate `BillingAuth::WalletSigned` and by lit-payments handlers that
/// need to look up the Stripe customer for the resolved wallet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedIdentity {
    /// `0x`-prefixed wallet address (lowercase).
    pub wallet_address_hex: String,
    /// `0x{keccak256_hex(walletAddress)}`. Same string the existing
    /// `usage_api_key_to_hash` helper detects via `is_precomputed_hash_shape`,
    /// so it can be passed straight to `resolve_wallet_address` downstream.
    pub api_key_hash_hex: String,
}

#[async_trait]
pub trait AuthResolver: Send + Sync + 'static {
    /// Verify an EIP-712 `BillingAuth` payload and return the recovered
    /// wallet address + derived hash, or an error.
    async fn verify_wallet_auth(
        &self,
        payload: &WalletAuthPayload,
    ) -> Result<ResolvedIdentity, AuthError>;

    /// Resolve a raw API key to its billing wallet address. The default
    /// implementation is intentionally absent — every consumer must wire one
    /// up (the resolver may need on-chain access or an HTTP hop).
    async fn resolve_api_key(&self, api_key: &str) -> Result<ResolvedIdentity, AuthError>;
}
