//! Request guard for billing endpoints (CPL-285).
//!
//! Accepts either form of authentication:
//!
//! 1. **API key** (legacy): the existing `Authorization: Bearer <key>` /
//!    `X-Api-Key: <key>` extraction. Identity = the raw key string, which the
//!    downstream `stripe::resolve_wallet_address` keccak256-hashes to derive
//!    the on-chain account.
//!
//! 2. **Wallet-signed** (ChainSecured): a SIWE-lite EIP-191 signed message in
//!    a single header `X-Wallet-Auth: <base64(JSON{message, signature})>`.
//!    The guard verifies the signature using the same security envelope as
//!    `create_wallet_with_signature` (chain-id match, ±5-minute timestamp
//!    skew). On success the wallet-derived `keccak256(walletAddress)` hex hash
//!    is the identity passed to `resolve_wallet_address`.
//!
//! This prevents the original CPL-285 weakness where the public wallet hash
//! alone was a bearer credential — anyone who knew the wallet address could
//! query balance and spawn PaymentIntents. The signature proves possession of
//! the wallet's private key.

use ethers::utils::keccak256;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use serde::Deserialize;

use crate::core::account_management::{SIWE_PURPOSE_BILLING_AUTH, verify_siwe_signature};
use crate::utils::parse_with_hash::is_precomputed_hash_shape;

/// Identity proven by an inbound billing request.
#[derive(Clone, Debug)]
pub enum BillingAuth {
    /// Raw API key (master or usage). Hashed by `resolve_wallet_address`.
    ApiKey(String),
    /// Successfully verified wallet signature. Carries the wallet-derived
    /// `keccak256(walletAddress)` hex hash for direct use as the identity
    /// string passed to `resolve_wallet_address` (no further hashing needed).
    WalletSigned {
        wallet_address_hex: String,
        api_key_hash_hex: String,
    },
}

impl BillingAuth {
    /// String to pass to `stripe::resolve_wallet_address`. For API-key flows
    /// this is the raw key (gets hashed downstream). For wallet-signed flows
    /// this is the precomputed `0x{keccak256_hex(walletAddress)}` — the
    /// `usage_api_key_to_hash` helper detects the precomputed-hash shape and
    /// uses it directly.
    pub fn identity_string(&self) -> &str {
        match self {
            BillingAuth::ApiKey(k) => k.as_str(),
            BillingAuth::WalletSigned {
                api_key_hash_hex, ..
            } => api_key_hash_hex.as_str(),
        }
    }
}

#[derive(Deserialize)]
struct WalletAuthPayload {
    message: String,
    signature: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BillingAuth {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<BillingAuth, Self::Error> {
        // Wallet-signed path takes precedence ONLY when the header parses
        // cleanly. A malformed X-Wallet-Auth (bad base64, bad JSON) falls
        // through to the API-key path so a junk-header proxy or stale
        // localStorage entry doesn't lock out an otherwise-valid API key.
        // A well-formed but signature-invalid payload IS a 401 — the caller
        // explicitly claimed wallet identity and we reject it.
        if let Some(encoded) = request.headers().get_one("X-Wallet-Auth") {
            let bytes = base64_light::base64_decode(encoded.trim());
            if !bytes.is_empty()
                && let Ok(payload) = serde_json::from_slice::<WalletAuthPayload>(&bytes)
            {
                match verify_siwe_signature(
                    &payload.message,
                    &payload.signature,
                    SIWE_PURPOSE_BILLING_AUTH,
                ) {
                    Ok(wallet) => {
                        let wallet_hex = format!("0x{:x}", wallet);
                        let hash = keccak256(wallet.as_bytes());
                        let api_key_hash_hex = format!("0x{}", hex::encode(hash));
                        return Outcome::Success(BillingAuth::WalletSigned {
                            wallet_address_hex: wallet_hex,
                            api_key_hash_hex,
                        });
                    }
                    Err(_) => return Outcome::Error((Status::Unauthorized, ())),
                }
            }
            // Bad base64 or bad JSON — fall through to the API-key path. We
            // log a warning so misbehaving clients are visible but don't
            // block legitimate API-key callers.
            tracing::warn!(
                "X-Wallet-Auth header present but unparseable; falling through to API key"
            );
        }

        // Legacy API-key path — same logic as `apikey::ApiKey`. Reject any
        // string shaped like a precomputed account hash here — those must
        // come through the verified WalletSigned path only. Otherwise an
        // attacker could send `X-Api-Key: 0x{keccak256(walletAddress)}` and
        // bypass SIWE entirely (CPL-285 hardening).
        let auth = request.headers().get_one("Authorization");
        if let Some(v) = auth {
            let v = v.trim();
            let mut parts = v.split_whitespace();
            if let (Some(scheme), Some(key_part)) = (parts.next(), parts.next())
                && scheme.eq_ignore_ascii_case("bearer")
            {
                let key = key_part.trim();
                if !key.is_empty() {
                    if is_precomputed_hash_shape(key) {
                        tracing::warn!(
                            "rejecting Authorization Bearer that looks like a precomputed account hash; \
                             ChainSecured callers must use X-Wallet-Auth"
                        );
                        return Outcome::Error((Status::Unauthorized, ()));
                    }
                    return Outcome::Success(BillingAuth::ApiKey(key.to_string()));
                }
            }
        }
        if let Some(key) = request.headers().get_one("X-Api-Key") {
            let key = key.trim();
            if !key.is_empty() {
                if is_precomputed_hash_shape(key) {
                    tracing::warn!(
                        "rejecting X-Api-Key that looks like a precomputed account hash; \
                         ChainSecured callers must use X-Wallet-Auth"
                    );
                    return Outcome::Error((Status::Unauthorized, ()));
                }
                return Outcome::Success(BillingAuth::ApiKey(key.to_string()));
            }
        }
        Outcome::Error((Status::Unauthorized, ()))
    }
}

impl<'r> OpenApiFromRequest<'r> for BillingAuth {
    fn from_request_input(
        generator: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        // BillingAuth accepts EITHER X-Api-Key OR X-Wallet-Auth — neither is
        // strictly required on its own. rocket_okapi 0.9 only supports
        // emitting one Parameter per guard, so we mark X-Api-Key as
        // optional and document the X-Wallet-Auth alternative in the
        // description. This prevents codegen (k6/openapi-to-k6) from
        // declaring `X-Api-Key: string` as a required field on
        // ChainSecured callers who never send it (CPL-285 review feedback).
        let schema = generator.json_schema::<String>();
        Ok(RequestHeaderInput::Parameter(Parameter {
            name: "X-Api-Key".to_owned(),
            location: "header".to_owned(),
            description: Some(
                "API-mode auth: account or usage API key (alternatively \
                 `Authorization: Bearer <key>`). OR — for ChainSecured \
                 callers — omit X-Api-Key entirely and send \
                 `X-Wallet-Auth: <base64(JSON{message, signature})>` where \
                 the message is a SIWE-lite EIP-191 payload with a \
                 `Purpose: lit-billing-auth-v1` line. The signature proves \
                 wallet possession; the signed message must include \
                 Address, Chain ID, and Issued At within ±5 minutes."
                    .to_owned(),
            ),
            required: false,
            deprecated: false,
            allow_empty_value: false,
            value: ParameterValue::Schema {
                style: None,
                explode: None,
                allow_reserved: false,
                schema,
                example: None,
                examples: None,
            },
            extensions: Object::default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_string_returns_raw_key_for_apikey_variant() {
        let auth = BillingAuth::ApiKey("my-raw-key".to_string());
        assert_eq!(auth.identity_string(), "my-raw-key");
    }

    #[test]
    fn identity_string_returns_hash_for_walletsigned_variant() {
        let auth = BillingAuth::WalletSigned {
            wallet_address_hex: "0x1111111111111111111111111111111111111111".to_string(),
            api_key_hash_hex: "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                .to_string(),
        };
        assert_eq!(
            auth.identity_string(),
            "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
    }

    /// CPL-285: a public wallet hash sent in `X-Api-Key` would have bypassed
    /// SIWE entirely after `usage_api_key_to_hash` started accepting the
    /// precomputed-hash form. The shape check is what blocks it. This test
    /// pins that decision so a future refactor can't silently undo it.
    #[test]
    fn precomputed_hash_strings_are_recognized() {
        // A 0x-prefixed 66-char keccak hash must be detectable. The actual
        // rejection happens in FromRequest::from_request, but the building
        // block is is_precomputed_hash_shape — verify it discriminates.
        let wallet_hash = "0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert!(is_precomputed_hash_shape(wallet_hash));
        // A real base64 API key (44 chars, base64 alphabet) does not match.
        let api_key = "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU2Nzg5";
        assert!(!is_precomputed_hash_shape(api_key));
    }
}
