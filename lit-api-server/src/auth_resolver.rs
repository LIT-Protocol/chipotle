//! Local [`AuthResolver`] implementation — runs auth verification in-process.
//!
//! `lit-api-server` already owns the on-chain `allApiKeyHashesToMaster`
//! resolver and the in-process EIP-712 verifier. This adapter wires those
//! into the shared `lit-billing-auth` trait so the same Rocket guard can
//! authenticate handlers on this service without an HTTP hop.
//!
//! The matching HTTP-based resolver lives in `lit-payments` and calls into
//! this service via `POST /internal/verify_wallet_auth` +
//! `POST /internal/resolve_api_key`.

use alloy::primitives::keccak256;
use async_trait::async_trait;
use lit_billing_auth::{AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload};

use crate::core::eip712::{PRIMARY_TYPE_BILLING_AUTH, verify_eip712_signature};

pub struct LocalAuthResolver;

impl LocalAuthResolver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalAuthResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthResolver for LocalAuthResolver {
    async fn verify_wallet_auth(
        &self,
        payload: &WalletAuthPayload,
    ) -> Result<ResolvedIdentity, AuthError> {
        let wallet = verify_eip712_signature(
            &payload.typed_data,
            &payload.signature,
            PRIMARY_TYPE_BILLING_AUTH,
        )
        .map_err(|e| {
            // ApiStatus has 4xx and 5xx variants; we collapse to BadCredentials
            // since every error path here is some flavour of "this payload did
            // not authenticate." Chain/RPC failures inside verify_eip712 would
            // be surfaced through the chain-id check, which is also a 4xx.
            AuthError::BadCredentials(format!("eip712 verification failed: {e:?}"))
        })?;

        let wallet_address_hex = format!("0x{:x}", wallet);
        let hash = keccak256(wallet.as_slice());
        let api_key_hash_hex = format!("0x{}", hex::encode(hash));
        Ok(ResolvedIdentity {
            wallet_address_hex,
            api_key_hash_hex,
        })
    }

    async fn resolve_api_key(&self, api_key: &str) -> Result<ResolvedIdentity, AuthError> {
        // `get_billing_wallet_address` already accepts either a raw API key or
        // a precomputed 0x-prefixed hash. The Rocket guard rejects the latter
        // for the API-key header (CPL-285 hardening), so callers here always
        // pass a raw key, but the function is shared with WalletSigned
        // downstream so we keep the broad acceptance.
        //
        // Codex P2 (Phase 2) fix: distinguish "key has no wallet on chain"
        // (BadCredentials → 401) from "RPC / contract call failed"
        // (Transient → 503). Pre-fix this method collapsed every error to
        // Transient, so bad API keys produced 503s and made lit-payments
        // retry-loop for permanent credential failures.
        let wallet_address_hex = match crate::accounts::get_billing_wallet_address(api_key).await {
            Ok(v) => v,
            Err(e) => {
                // `accounts::get_billing_wallet_address` bails with "account
                // has no wallet address" when the on-chain mapping returns
                // Address::ZERO for this key hash — i.e. the key isn't
                // registered. Any other anyhow error is transport / RPC /
                // contract decoding noise → transient.
                let msg = format!("{e}");
                if msg.contains("no wallet address") || msg.contains("Address::ZERO") {
                    return Err(AuthError::BadCredentials(format!(
                        "api key not found on chain: {msg}"
                    )));
                }
                return Err(AuthError::Transient(format!(
                    "on-chain resolve failed: {msg}"
                )));
            }
        };

        // Derive the api_key_hash_hex from the wallet address — same value
        // ChainSecured callers carry around so downstream caching keys match.
        let bytes = hex::decode(wallet_address_hex.trim_start_matches("0x"))
            .map_err(|e| AuthError::BadCredentials(format!("invalid wallet hex: {e}")))?;
        let hash = keccak256(&bytes);
        let api_key_hash_hex = format!("0x{}", hex::encode(hash));

        Ok(ResolvedIdentity {
            wallet_address_hex,
            api_key_hash_hex,
        })
    }
}
