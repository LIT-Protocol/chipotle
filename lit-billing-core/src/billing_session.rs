//! Long-lived billing sessions for wallets that cannot re-sign every few
//! minutes, such as a Safe multisig.
//!
//! A short-lived `BillingAuth` signature is a bearer credential: whoever holds
//! the signed typed data can call billing endpoints until it expires five
//! minutes later. That window cannot simply be stretched to days, because a
//! Safe's off-chain message signatures are published on the Safe Transaction
//! Service, where anyone can list them.
//!
//! A `BillingSession` instead delegates to a session key. The dashboard
//! generates a throwaway key pair, asks the wallet to sign a typed-data struct
//! naming the key's address, and keeps the private key in the browser. Every
//! request then carries two things in `X-Wallet-Auth`:
//!
//! 1. the wallet's `BillingSession` signature (valid for
//!    [`BILLING_SESSION_MAX_AGE_SECONDS`] from `issuedAt`), and
//! 2. a [`SessionProof`]: an EIP-191 signature by the session key over the
//!    current timestamp (valid for [`TIMESTAMP_SKEW_SECONDS`]).
//!
//! The first proves the wallet authorised the session key; the second proves
//! the caller holds that key. A published wallet signature is useless without
//! the session key.

use alloy_primitives::{Address, Signature, eip191_hash_message};

use crate::billing_auth::{SessionProof, WalletAuthPayload};
use crate::eip712::{
    Eip712Error, Erc1271Verifier, PRIMARY_TYPE_BILLING_AUTH, PRIMARY_TYPE_BILLING_SESSION,
    TIMESTAMP_SKEW_SECONDS, unix_now, verify_billing_session_allow_contract_wallet,
    verify_eip712_signature_allow_contract_wallet,
};

/// The exact text a session key signs (EIP-191 `personal_sign`) for one
/// request. The dashboard builds the same string; keep both in sync.
pub fn session_proof_message(session_key: Address, timestamp: i64) -> String {
    format!(
        "Lit ChainSecured billing request\nSession key: {}\nTimestamp: {timestamp}",
        session_key.to_checksum(None)
    )
}

/// Check that `proof` is a fresh signature by `session_key`.
pub fn verify_session_proof(session_key: Address, proof: &SessionProof) -> Result<(), Eip712Error> {
    verify_session_proof_at(session_key, proof, unix_now()?)
}

fn verify_session_proof_at(
    session_key: Address,
    proof: &SessionProof,
    now: i64,
) -> Result<(), Eip712Error> {
    let delta = (now as i128) - (proof.timestamp as i128);
    if delta.abs() > TIMESTAMP_SKEW_SECONDS as i128 {
        return Err(Eip712Error::bad(
            format!(
                "Session proof timestamp {} is outside the ±{TIMESTAMP_SKEW_SECONDS}s window from now ({now})",
                proof.timestamp
            ),
            "Session proof timestamp is too old or too far in the future",
        ));
    }
    let sig_str = proof.signature.trim();
    let sig_bytes = hex::decode(sig_str.strip_prefix("0x").unwrap_or(sig_str))
        .map_err(|e| Eip712Error::bad(e.to_string(), "Invalid session proof signature hex"))?;
    if sig_bytes.len() != 65 {
        return Err(Eip712Error::bad(
            format!(
                "session proof signature is {} bytes, expected 65",
                sig_bytes.len()
            ),
            "Invalid session proof signature",
        ));
    }
    let signature = Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| Eip712Error::bad(e.to_string(), "Invalid session proof signature"))?;
    let digest = eip191_hash_message(session_proof_message(session_key, proof.timestamp));
    let recovered = signature
        .recover_address_from_prehash(&digest)
        .map_err(|e| Eip712Error::bad(e.to_string(), "Invalid session proof signature"))?;
    if recovered != session_key {
        return Err(Eip712Error::bad(
            "Session proof was not signed by the session key",
            "Session proof was not signed by the session key",
        ));
    }
    Ok(())
}

/// The `primaryType` of an `X-Wallet-Auth` payload's typed data, accepting the
/// object form and the stringified form wallets sometimes return.
fn payload_primary_type(typed_data: &serde_json::Value) -> Option<String> {
    let field = |v: &serde_json::Value| v.get("primaryType")?.as_str().map(str::to_owned);
    match typed_data {
        serde_json::Value::String(s) => serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .as_ref()
            .and_then(field),
        other => field(other),
    }
}

/// Verify an `X-Wallet-Auth` payload and return the authenticated wallet.
///
/// - `BillingSession` payloads must carry a valid [`SessionProof`] from the
///   session key the wallet signed.
/// - Anything else is treated as a short-lived `BillingAuth` signature, as
///   before.
///
/// Both paths accept EOA signatures and EIP-1271 contract-wallet signatures.
pub async fn verify_wallet_auth_payload(
    payload: &WalletAuthPayload,
    expected_chain_id: u64,
    verifier: &dyn Erc1271Verifier,
) -> Result<Address, Eip712Error> {
    if payload_primary_type(&payload.typed_data).as_deref() != Some(PRIMARY_TYPE_BILLING_SESSION) {
        return verify_eip712_signature_allow_contract_wallet(
            &payload.typed_data,
            &payload.signature,
            PRIMARY_TYPE_BILLING_AUTH,
            expected_chain_id,
            verifier,
        )
        .await;
    }
    let proof = payload.session_proof.as_ref().ok_or_else(|| {
        Eip712Error::bad(
            "BillingSession payload has no session_proof",
            "Billing session requests must include a session proof",
        )
    })?;
    // Check the proof before the wallet signature: it is local and cheap,
    // while the wallet check may cost an EIP-1271 RPC call. The session key
    // comes from the typed data, so read it without trusting anything else.
    let session_key = payload_session_key(&payload.typed_data)?;
    verify_session_proof(session_key, proof)?;
    let claims = verify_billing_session_allow_contract_wallet(
        &payload.typed_data,
        &payload.signature,
        expected_chain_id,
        verifier,
    )
    .await?;
    // `payload_session_key` and the verifier parse the same JSON; this guards
    // against the two ever disagreeing.
    if claims.session_key != session_key {
        return Err(Eip712Error::bad(
            "session key mismatch between proof check and typed data",
            "Session proof was not signed by the session key",
        ));
    }
    Ok(claims.wallet)
}

fn payload_session_key(typed_data: &serde_json::Value) -> Result<Address, Eip712Error> {
    let parsed;
    let typed_data = match typed_data {
        serde_json::Value::String(s) => {
            parsed = serde_json::from_str::<serde_json::Value>(s).map_err(|e| {
                Eip712Error::bad(e.to_string(), "typed_data is a string but not valid JSON")
            })?;
            &parsed
        }
        other => other,
    };
    let value = typed_data
        .get("message")
        .and_then(|m| m.get("sessionKey"))
        .ok_or_else(|| {
            Eip712Error::bad("Missing message.sessionKey", "Missing message.sessionKey")
        })?;
    serde_json::from_value(value.clone())
        .map_err(|e| Eip712Error::bad(e.to_string(), "Invalid message.sessionKey"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::TypedData;
    use alloy_primitives::B256;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use std::sync::atomic::{AtomicBool, Ordering};

    const CHAIN_ID: u64 = 8453;

    fn now() -> i64 {
        unix_now().unwrap()
    }

    fn session_typed_data(
        wallet: Address,
        session_key: Address,
        issued_at: i64,
    ) -> serde_json::Value {
        serde_json::json!({
            "types": {
                "EIP712Domain": [
                    {"name": "name", "type": "string"},
                    {"name": "version", "type": "string"},
                    {"name": "chainId", "type": "uint256"},
                ],
                "BillingSession": [
                    {"name": "address", "type": "address"},
                    {"name": "sessionKey", "type": "address"},
                    {"name": "issuedAt", "type": "uint256"},
                ],
            },
            "primaryType": "BillingSession",
            "domain": {"name": "Lit ChainSecured", "version": "1", "chainId": CHAIN_ID.to_string()},
            "message": {
                "address": format!("{wallet:#x}"),
                "sessionKey": format!("{session_key:#x}"),
                "issuedAt": issued_at.to_string(),
            },
        })
    }

    fn sign_typed(signer: &PrivateKeySigner, typed: &serde_json::Value) -> String {
        let td: TypedData = serde_json::from_value(typed.clone()).unwrap();
        let sig = signer
            .sign_hash_sync(&td.eip712_signing_hash().unwrap())
            .unwrap();
        format!("0x{}", hex::encode(sig.as_bytes()))
    }

    fn proof(session: &PrivateKeySigner, timestamp: i64) -> SessionProof {
        let msg = session_proof_message(session.address(), timestamp);
        let sig = session.sign_message_sync(msg.as_bytes()).unwrap();
        SessionProof {
            timestamp,
            signature: format!("0x{}", hex::encode(sig.as_bytes())),
        }
    }

    struct Mock {
        accept: bool,
        called: AtomicBool,
    }
    impl Mock {
        fn new(accept: bool) -> Self {
            Self {
                accept,
                called: AtomicBool::new(false),
            }
        }
    }
    #[async_trait::async_trait]
    impl Erc1271Verifier for Mock {
        async fn verify_erc1271(&self, _: Address, _: B256, _: &[u8]) -> Result<bool, Eip712Error> {
            self.called.store(true, Ordering::SeqCst);
            Ok(self.accept)
        }
    }

    fn payload(
        typed: serde_json::Value,
        signature: String,
        proof: Option<SessionProof>,
    ) -> WalletAuthPayload {
        WalletAuthPayload {
            typed_data: typed,
            signature,
            session_proof: proof,
        }
    }

    #[tokio::test]
    async fn eoa_session_with_fresh_proof_verifies() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let typed = session_typed_data(wallet.address(), session.address(), now());
        let sig = sign_typed(&wallet, &typed);
        let got = verify_wallet_auth_payload(
            &payload(typed, sig, Some(proof(&session, now()))),
            CHAIN_ID,
            &Mock::new(false),
        )
        .await
        .unwrap();
        assert_eq!(got, wallet.address());
    }

    /// The point of the feature: a multisig session signed days ago still
    /// works, as long as the caller holds the session key.
    #[tokio::test]
    async fn contract_wallet_session_signed_days_ago_verifies() {
        let safe = Address::repeat_byte(0x5a);
        let session = PrivateKeySigner::random();
        let typed = session_typed_data(safe, session.address(), now() - 5 * 24 * 3600);
        let verifier = Mock::new(true);
        let got = verify_wallet_auth_payload(
            &payload(
                typed,
                format!("0x{}", "ab".repeat(130)),
                Some(proof(&session, now())),
            ),
            CHAIN_ID,
            &verifier,
        )
        .await
        .unwrap();
        assert_eq!(got, safe);
        assert!(verifier.called.load(Ordering::SeqCst));
    }

    /// A published wallet signature without the session key is useless.
    #[tokio::test]
    async fn session_without_proof_is_rejected() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let typed = session_typed_data(wallet.address(), session.address(), now());
        let sig = sign_typed(&wallet, &typed);
        let err =
            verify_wallet_auth_payload(&payload(typed, sig, None), CHAIN_ID, &Mock::new(true))
                .await
                .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
    }

    #[tokio::test]
    async fn proof_from_another_key_is_rejected_before_any_chain_call() {
        let session = PrivateKeySigner::random();
        let attacker = PrivateKeySigner::random();
        let typed = session_typed_data(Address::repeat_byte(0x5a), session.address(), now());
        let verifier = Mock::new(true);
        let err = verify_wallet_auth_payload(
            &payload(
                typed,
                format!("0x{}", "ab".repeat(130)),
                Some(proof(&attacker, now())),
            ),
            CHAIN_ID,
            &verifier,
        )
        .await
        .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
        assert!(!verifier.called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stale_proof_is_rejected() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let typed = session_typed_data(wallet.address(), session.address(), now());
        let sig = sign_typed(&wallet, &typed);
        let stale = proof(&session, now() - TIMESTAMP_SKEW_SECONDS - 60);
        let err = verify_wallet_auth_payload(
            &payload(typed, sig, Some(stale)),
            CHAIN_ID,
            &Mock::new(false),
        )
        .await
        .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
    }

    #[tokio::test]
    async fn expired_session_is_rejected() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let typed = session_typed_data(
            wallet.address(),
            session.address(),
            now() - crate::eip712::BILLING_SESSION_MAX_AGE_SECONDS - 60,
        );
        let sig = sign_typed(&wallet, &typed);
        let err = verify_wallet_auth_payload(
            &payload(typed, sig, Some(proof(&session, now()))),
            CHAIN_ID,
            &Mock::new(false),
        )
        .await
        .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
    }

    #[tokio::test]
    async fn future_session_is_rejected() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let typed = session_typed_data(wallet.address(), session.address(), now() + 3600);
        let sig = sign_typed(&wallet, &typed);
        let err = verify_wallet_auth_payload(
            &payload(typed, sig, Some(proof(&session, now()))),
            CHAIN_ID,
            &Mock::new(false),
        )
        .await
        .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
    }

    /// The session key is part of the signed struct: swapping it breaks the
    /// wallet signature even when the proof matches the swapped key.
    #[tokio::test]
    async fn substituted_session_key_breaks_wallet_signature() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let attacker = PrivateKeySigner::random();
        let typed = session_typed_data(wallet.address(), session.address(), now());
        let sig = sign_typed(&wallet, &typed);
        let mut forged = typed.clone();
        forged["message"]["sessionKey"] = serde_json::json!(format!("{:#x}", attacker.address()));
        let err = verify_wallet_auth_payload(
            &payload(forged, sig, Some(proof(&attacker, now()))),
            CHAIN_ID,
            &Mock::new(false),
        )
        .await
        .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
    }

    /// The legacy five-minute `BillingAuth` path is unchanged.
    #[tokio::test]
    async fn legacy_billing_auth_still_verifies() {
        let wallet = PrivateKeySigner::random();
        let typed = crate::eip712::build_canonical_typed_data_json(
            PRIMARY_TYPE_BILLING_AUTH,
            wallet.address(),
            now(),
            CHAIN_ID,
        );
        let sig = sign_typed(&wallet, &typed);
        let got =
            verify_wallet_auth_payload(&payload(typed, sig, None), CHAIN_ID, &Mock::new(false))
                .await
                .unwrap();
        assert_eq!(got, wallet.address());
    }

    /// A session-shaped struct cannot be smuggled in under the legacy name to
    /// skip the proof: `BillingAuth` pins the two-field schema.
    #[tokio::test]
    async fn session_fields_under_billing_auth_name_are_rejected() {
        let wallet = PrivateKeySigner::random();
        let session = PrivateKeySigner::random();
        let mut typed = session_typed_data(wallet.address(), session.address(), now());
        let fields = typed["types"]["BillingSession"].clone();
        typed["types"] = serde_json::json!({
            "EIP712Domain": typed["types"]["EIP712Domain"].clone(),
            "BillingAuth": fields,
        });
        typed["primaryType"] = serde_json::json!("BillingAuth");
        let sig = sign_typed(&wallet, &typed);
        let err =
            verify_wallet_auth_payload(&payload(typed, sig, None), CHAIN_ID, &Mock::new(false))
                .await
                .unwrap_err();
        assert!(err.is_bad_request(), "{err}");
    }

    #[test]
    fn proof_message_is_stable() {
        let key: Address = "0x00000000000000000000000000000000000000aa"
            .parse()
            .unwrap();
        assert_eq!(
            session_proof_message(key, 1_794_319_086),
            "Lit ChainSecured billing request\nSession key: 0x00000000000000000000000000000000000000AA\nTimestamp: 1794319086"
        );
    }
}
