//! Local [`AuthResolver`] implementation — runs auth verification in-process.
//!
//! `lit-api-server` already owns the on-chain `allApiKeyHashesToMaster`
//! resolver and the in-process EIP-712 verifier. This adapter wires those
//! into the shared `lit-billing-core::billing_auth` trait so the same Rocket
//! guard can authenticate handlers on this service without an HTTP hop.
//!
//! Post-glitch-refactor: `lit-payments` constructs an identical in-process
//! resolver using the shared crate's primitives — see
//! `lit-payments/src/auth_resolver.rs`. There is no HTTP fallback any more;
//! the `/internal/verify_wallet_auth` + `/internal/resolve_api_key`
//! endpoints have been deleted.

use alloy::primitives::keccak256;
use async_trait::async_trait;
use lit_billing_core::billing_auth::{
    AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload,
};

use crate::core::eip712::{
    PRIMARY_TYPE_BILLING_AUTH, verify_eip712_signature_allow_contract_wallet,
};

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
        // Accept both EOA (65-byte ECDSA) and EIP-1271 smart-contract-wallet
        // signatures, so a ChainSecured account whose admin is a smart wallet
        // (e.g. a ZeroDev Kernel owned by a passkey) can authenticate to billing
        // — symmetric with the account-management mint endpoints, which already
        // use this verifier. The EIP-1271 path does an on-chain
        // `isValidSignature` call via the read-only client on the node's
        // configured chain; the EOA path short-circuits before any RPC.
        let wallet = verify_eip712_signature_allow_contract_wallet(
            &payload.typed_data,
            &payload.signature,
            PRIMARY_TYPE_BILLING_AUTH,
        )
        .await
        .map_err(|e| {
            // ApiStatus carries the HTTP status. A 5xx is an infra failure
            // (read-only client unavailable, RPC timeout on the EIP-1271 call)
            // — that's Transient (503) so lit-payments/the dashboard can retry,
            // not a credential rejection. Everything else (bad payload, wrong
            // domain/chain/schema, stale timestamp, signature mismatch) is a
            // 4xx → BadCredentials (401).
            if e.status.code >= 500 {
                AuthError::Transient(format!("eip712 verification infra error: {e:?}"))
            } else {
                AuthError::BadCredentials(format!("eip712 verification failed: {e:?}"))
            }
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
                // An unregistered key surfaces two ways depending on the
                // AccountConfig contract version:
                //  - Older contract: `get_billing_wallet_address` returns
                //    Address::ZERO and bails with "account has no wallet
                //    address".
                //  - Post-#481 contract: the call reverts with
                //    `AccountDoesNotExist (0xd4a84737)`, which
                //    `decode_contract_revert` surfaces as
                //    "Contract error: AccountDoesNotExist (...)".
                // Both are permanent credential failures → BadCredentials
                // (401) so lit-payments stops retrying. Any other anyhow error
                // is transport / RPC / contract decoding noise → transient.
                let msg = format!("{e}");
                if msg.contains("no wallet address")
                    || msg.contains("Address::ZERO")
                    || msg.contains("AccountDoesNotExist")
                {
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
