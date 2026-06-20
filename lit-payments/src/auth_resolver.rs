//! In-process [`AuthResolver`] for lit-payments.
//!
//! Pre-glitch-refactor this was an HTTP resolver that forwarded
//! wallet-sig verification and API-key resolution to lit-api-server's
//! `/internal/verify_wallet_auth` + `/internal/resolve_api_key`
//! endpoints. Glitch's PR #448 review flagged that hop as plumbing
//! dressed up as architecture: both operations are pure-Rust
//! (EIP-712 verify) or single-eth_call (on-chain key resolution) and
//! exposing them across a service boundary added latency, an
//! `X-Internal-Secret` attack surface, and a coupling between
//! lit-payments availability and lit-api-server availability.
//!
//! After folding the verifier into `lit-billing-core::eip712` and
//! adding `lit-billing-core::on_chain::OnChainBillingResolver`,
//! lit-payments now runs the same logic in-process. Symmetric with
//! `lit-api-server`'s `LocalAuthResolver` — same primitives, same
//! behaviour, same error classification.

use alloy_primitives::{Address, keccak256};
use async_trait::async_trait;
use lit_billing_core::billing_auth::{
    AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload,
};
use lit_billing_core::eip712::{
    Eip712Error, PRIMARY_TYPE_BILLING_AUTH, verify_eip712_signature_allow_contract_wallet,
};
use lit_billing_core::on_chain::{OnChainBillingResolver, ResolveError};

/// Resolver wiring: an on-chain resolver pre-built from env config and the
/// chain id the EIP-712 verifier should commit to. Constructed once at
/// startup and shared via `Arc<dyn AuthResolver>` in Rocket state.
pub struct LocalAuthResolver {
    on_chain: OnChainBillingResolver,
    chain_id: u64,
}

impl LocalAuthResolver {
    /// Build a resolver. `chain_id` is the chain the EIP-712 wallet-sig
    /// must commit to; should match the chain `on_chain` reads from.
    pub fn new(on_chain: OnChainBillingResolver, chain_id: u64) -> Self {
        Self { on_chain, chain_id }
    }
}

#[async_trait]
impl AuthResolver for LocalAuthResolver {
    async fn verify_wallet_auth(
        &self,
        payload: &WalletAuthPayload,
    ) -> Result<ResolvedIdentity, AuthError> {
        // Accept both EOA (65-byte ECDSA) and EIP-1271 smart-contract-wallet
        // signatures. A ChainSecured account whose admin is a smart wallet
        // (e.g. a ZeroDev Kernel owned by a passkey) must be able to
        // authenticate to billing too, not just mint wallets — `self.on_chain`
        // supplies the on-chain `isValidSignature` check, on the same chain the
        // EIP-712 domain pins.
        let wallet = verify_eip712_signature_allow_contract_wallet(
            &payload.typed_data,
            &payload.signature,
            PRIMARY_TYPE_BILLING_AUTH,
            self.chain_id,
            &self.on_chain,
        )
        .await
        .map_err(map_eip712_error)?;

        Ok(identity_for_wallet(wallet))
    }

    async fn resolve_api_key(&self, api_key: &str) -> Result<ResolvedIdentity, AuthError> {
        // `OnChainBillingResolver` accepts either a raw API key or a
        // precomputed 0x-prefixed hash. The Rocket guard rejects the
        // latter for the API-key header (CPL-285 hardening), so the
        // callers we see always pass raw keys; the function's broad
        // acceptance is preserved for symmetry with the lit-api-server
        // `LocalAuthResolver` and for any non-guard caller.
        let wallet_hex = match self.on_chain.get_billing_wallet_address(api_key).await {
            Ok(v) => v,
            Err(ResolveError::NotFound) => {
                return Err(AuthError::BadCredentials(
                    "api key not found on chain".to_string(),
                ));
            }
            Err(ResolveError::Transient(msg)) => {
                return Err(AuthError::Transient(format!(
                    "on-chain resolve failed: {msg}"
                )));
            }
        };

        // Decode the wallet hex into bytes so we can derive the canonical
        // api_key_hash_hex. The on-chain resolver always returns a
        // lowercase 0x-prefixed 20-byte hex string.
        let bytes = hex::decode(wallet_hex.trim_start_matches("0x"))
            .map_err(|e| AuthError::BadCredentials(format!("invalid wallet hex: {e}")))?;
        if bytes.len() != 20 {
            return Err(AuthError::Transient(format!(
                "on-chain resolver returned non-20-byte address: {wallet_hex}"
            )));
        }
        let mut arr = [0u8; 20];
        arr.copy_from_slice(&bytes);
        let address = Address::from(arr);
        Ok(identity_for_wallet(address))
    }
}

/// Translate a shared `Eip712Error` into the auth-trait `AuthError`.
/// `BadRequest` ↦ `BadCredentials` (401); `Internal` ↦ `Transient` (503).
/// Matches the lit-api-server `LocalAuthResolver` mapping so behaviour is
/// identical on both services.
fn map_eip712_error(e: Eip712Error) -> AuthError {
    if e.is_bad_request() {
        AuthError::BadCredentials(format!("eip712 verification failed: {e}"))
    } else {
        AuthError::Transient(format!("eip712 internal: {e}"))
    }
}

/// Build the canonical `(wallet_address_hex, api_key_hash_hex)` pair from
/// a recovered `Address`. The `api_key_hash_hex` is the precomputed
/// 0x-prefixed keccak256 of the wallet bytes — same value ChainSecured
/// callers route under, so downstream caching keys match across both
/// authentication paths.
fn identity_for_wallet(wallet: Address) -> ResolvedIdentity {
    let wallet_address_hex = format!("{wallet:#x}");
    let hash = keccak256(wallet.as_slice());
    let api_key_hash_hex = format!("0x{}", hex::encode(hash));
    ResolvedIdentity {
        wallet_address_hex,
        api_key_hash_hex,
    }
}
