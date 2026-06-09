//! EIP-712 verifier adapter for `lit-api-server`.
//!
//! The actual implementation lives in `lit_billing_core::eip712` so both
//! `lit-api-server` (this crate's account-management endpoints) and
//! `lit-payments` (the `BillingAuth` Rocket guard) can call it directly
//! without an HTTP hop. This module is just the lit-api-server-specific
//! adapter:
//!
//! 1. Read the server's `chain_id` from `GLOBAL_NODE_CONFIG` (the shared
//!    verifier takes it as a parameter — no global state).
//! 2. Translate `Eip712Error` back into `ApiStatus` so existing callers
//!    keep their `Result<_, ApiStatus>` signatures unchanged.
//! 3. Re-export the canonical `PRIMARY_TYPE_*` constants under the same
//!    paths callers already use (`crate::core::eip712::PRIMARY_TYPE_*`).

use alloy::primitives::Address;

use crate::config::GLOBAL_NODE_CONFIG;
use crate::core::v1::helpers::api_status::ApiStatus;

// Re-export shared primary-type constants and helpers so existing
// `use crate::core::eip712::*` paths keep working.
pub use lit_billing_core::eip712::{
    PRIMARY_TYPE_ADD_USAGE_API_KEY, PRIMARY_TYPE_BILLING_AUTH, PRIMARY_TYPE_CONVERT_ACCOUNT,
    PRIMARY_TYPE_CREATE_WALLET,
};

/// Verify an EIP-712 typed-data + signature pair, using this service's
/// configured chain_id. Thin wrapper over `lit_billing_core::eip712::verify_eip712_signature`
/// that surfaces errors as `ApiStatus` for the existing Rocket handlers.
pub fn verify_eip712_signature(
    typed_data_json: &serde_json::Value,
    signature_hex: &str,
    expected_primary_type: &str,
) -> Result<Address, ApiStatus> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))
        .map_err(|e| ApiStatus::internal_server_error(e, "GLOBAL_NODE_CONFIG missing"))?;
    let chain_id = node_config.chain.info().chain_id;

    lit_billing_core::eip712::verify_eip712_signature(
        typed_data_json,
        signature_hex,
        expected_primary_type,
        chain_id,
    )
    .map_err(map_err)
}

/// Translate the shared verifier's `Eip712Error` into `ApiStatus`. The
/// shared crate has no notion of `ApiStatus` (it lives in lit-api-server's
/// core/v1/helpers) so we do the mapping here once.
fn map_err(e: lit_billing_core::eip712::Eip712Error) -> ApiStatus {
    let summary = e.summary().to_string();
    if e.is_bad_request() {
        ApiStatus::bad_request(anyhow::anyhow!("{e}"), summary)
    } else {
        ApiStatus::internal_server_error(anyhow::anyhow!("{e}"), summary)
    }
}
