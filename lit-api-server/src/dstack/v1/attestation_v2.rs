//! GET /attestation/v2 — a fresh, KMS-certified attestation statement.
//!
//! `/attestation` returns a TDX quote and its event log. A verifier can check
//! the quote's Intel signature and replay the event log, but the identity
//! events it reads (app-id, compose-hash, os-image-hash, key-provider) are
//! written by the guest OS itself. Nothing in that response proves the guest
//! booted the approved OS image, and the quote carries no caller nonce.
//!
//! This endpoint adds both:
//!
//! - **Boot binding.** dstack's KMS releases an app's keys only after checking
//!   the requesting CVM's boot measurements against the whitelisted OS image,
//!   its compose hash and its app id. We derive a dedicated secp256k1 signer
//!   from those keys (a new path, not keccak-post-processed like the other
//!   keys, so the chain covers it directly) and return dstack's signature chain:
//!   `kms_signature` (KMS root over the app key) and `app_signature` (app key
//!   over this signer). A verifier checks the chain against the KMS key
//!   published on-chain in DstackKms, so only a genuine, governed CVM can
//!   produce a valid signer.
//! - **Freshness.** The signer signs the caller's 32-byte nonce together with
//!   the hash of the quote served in the same response, so a response cannot
//!   be replayed or paired with another machine's quote. The quote itself is
//!   cached (refreshed hourly): a request costs one signature, never a quote.
//!
//! The signer is used for nothing else. Its derivation path and purpose are
//! new, so no existing key, wallet or address changes. `/attestation` and
//! `/info` are untouched for older SDKs.

use k256::ecdsa::SigningKey;
use rocket::get;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::Serialize;
use sha3::{Digest, Keccak256};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OnceCell};

use super::dstack;

/// dstack key derivation path for the attestation signer. Never reuse it.
pub const SIGNER_PATH: &str = "chipotle/attestation/v2/signer";
/// dstack key purpose; covered by the app key's signature over the signer.
pub const SIGNER_PURPOSE: &str = "chipotle-attestation-v2";
/// Domain separator for the per-request statement.
pub const STATEMENT_DOMAIN: &[u8] = b"chipotle-attestation-v2:";

#[derive(Debug, Serialize)]
pub struct AttestationV2Response {
    pub version: u32,
    /// Echo of the caller's nonce (64 lowercase hex characters).
    pub nonce: String,
    pub quote: String,
    pub event_log: String,
    pub vm_config: String,
    pub signer: SignerInfo,
    /// 65-byte recoverable secp256k1 signature (r || s || v), hex, over
    /// keccak256(STATEMENT_DOMAIN || nonce || keccak256(quote bytes)).
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignerInfo {
    /// Compressed SEC1 secp256k1 public key, hex.
    pub public_key: String,
    pub purpose: String,
    /// App key over `purpose:public_key` (65 bytes, hex).
    pub app_signature: String,
    /// KMS root key over the app key (65 bytes, hex).
    pub kms_signature: String,
}

struct Signer {
    key: SigningKey,
    info: SignerInfo,
}

static SIGNER: OnceCell<Signer> = OnceCell::const_new();

/// The quote and event log served with every signature, cached for
/// `QUOTE_TTL` so the event log stays current without a quote per request. A
/// request that finds the cache empty or expired generates the replacement
/// through the dstack agent while holding the lock, so concurrent requests wait
/// for that one refresh instead of each generating a quote.
struct CachedQuote {
    quote: String,
    quote_bytes: Vec<u8>,
    event_log: String,
    vm_config: String,
}
const QUOTE_TTL: Duration = Duration::from_secs(3600);
static QUOTE: Mutex<Option<(Instant, Arc<CachedQuote>)>> = Mutex::const_new(None);

/// Parses a 32-byte nonce given as exactly 64 lowercase hex characters.
pub fn parse_nonce(nonce: &str) -> Option<[u8; 32]> {
    if nonce.len() != 64
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(nonce, &mut out).ok()?;
    Some(out)
}

/// The digest the signer signs for one response.
pub fn statement_digest(nonce: &[u8; 32], quote: &[u8]) -> Keccak256 {
    let quote_hash = Keccak256::digest(quote);
    let mut digest = Keccak256::new();
    digest.update(STATEMENT_DOMAIN);
    digest.update(nonce);
    digest.update(quote_hash);
    digest
}

/// 65-byte r || s || v signature over the statement for `nonce` and `quote`.
pub fn sign_statement(key: &SigningKey, nonce: &[u8; 32], quote: &[u8]) -> Result<Vec<u8>, String> {
    let (signature, recovery_id) = key
        .sign_digest_recoverable(statement_digest(nonce, quote))
        .map_err(|e| format!("failed to sign attestation statement: {e}"))?;
    let mut out = signature.to_vec();
    out.push(recovery_id.to_byte());
    Ok(out)
}

/// Builds the signer from a dstack GetKey response, rejecting anything that is
/// not a secp256k1 key with a two-link signature chain.
fn signer_from_key_response(key: &[u8], chain: &[Vec<u8>]) -> Result<Signer, String> {
    let key = SigningKey::from_slice(key)
        .map_err(|e| format!("attestation signer is not a secp256k1 key: {e}"))?;
    if chain.len() != 2 || chain.iter().any(|s| s.len() != 65) {
        return Err("attestation signer has no KMS signature chain".to_string());
    }
    let public_key = hex::encode(key.verifying_key().to_sec1_bytes());
    Ok(Signer {
        key,
        info: SignerInfo {
            public_key,
            purpose: SIGNER_PURPOSE.to_string(),
            app_signature: hex::encode(&chain[0]),
            kms_signature: hex::encode(&chain[1]),
        },
    })
}

async fn signer() -> Result<&'static Signer, String> {
    SIGNER
        .get_or_try_init(|| async {
            let response = dstack::get_key(SIGNER_PATH, SIGNER_PURPOSE).await?;
            let key = response
                .decode_key()
                .map_err(|e| format!("failed to decode attestation signer: {e}"))?;
            let chain = response
                .decode_signature_chain()
                .map_err(|e| format!("failed to decode signature chain: {e}"))?;
            signer_from_key_response(&key, &chain)
        })
        .await
}

async fn cached_quote() -> Result<Arc<CachedQuote>, String> {
    let mut slot = QUOTE.lock().await;
    if let Some((fetched, quote)) = slot.as_ref()
        && fetched.elapsed() < QUOTE_TTL
    {
        return Ok(quote.clone());
    }
    let quote = dstack::get_quote(None).await?;
    let quote_bytes = hex::decode(quote.quote.trim().trim_start_matches("0x"))
        .map_err(|e| format!("dstack returned a non-hex quote: {e}"))?;
    let cached = Arc::new(CachedQuote {
        quote: quote.quote,
        quote_bytes,
        event_log: quote.event_log,
        vm_config: quote.vm_config,
    });
    *slot = Some((Instant::now(), cached.clone()));
    Ok(cached)
}

/// GET /attestation/v2?nonce=<64 lowercase hex>
#[get("/attestation/v2?<nonce>")]
pub async fn attestation_v2(
    nonce: Option<&str>,
) -> Result<Json<AttestationV2Response>, (Status, String)> {
    let nonce_hex = nonce.unwrap_or_default();
    let nonce_bytes = parse_nonce(nonce_hex).ok_or((
        Status::BadRequest,
        "nonce must be 32 bytes as 64 lowercase hex characters".to_string(),
    ))?;
    let signer = signer()
        .await
        .map_err(|e| (Status::ServiceUnavailable, e))?;
    let quote = cached_quote()
        .await
        .map_err(|e| (Status::ServiceUnavailable, e))?;
    let signature = sign_statement(&signer.key, &nonce_bytes, &quote.quote_bytes)
        .map_err(|e| (Status::ServiceUnavailable, e))?;
    Ok(Json(AttestationV2Response {
        version: 2,
        nonce: nonce_hex.to_string(),
        quote: quote.quote.clone(),
        event_log: quote.event_log.clone(),
        vm_config: quote.vm_config.clone(),
        signer: signer.info.clone(),
        signature: hex::encode(signature),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};

    const KEY: [u8; 32] = [7u8; 32];

    #[test]
    fn nonce_must_be_exactly_32_lowercase_hex_bytes() {
        let good = "00".repeat(31) + "ff";
        assert_eq!(parse_nonce(&good).unwrap()[31], 0xff);
        assert!(parse_nonce("").is_none());
        assert!(parse_nonce(&"00".repeat(31)).is_none());
        assert!(parse_nonce(&"00".repeat(33)).is_none());
        assert!(parse_nonce(&("00".repeat(31) + "FF")).is_none());
        assert!(parse_nonce(&("00".repeat(31) + "zz")).is_none());
        assert!(parse_nonce(&("0x".to_string() + &"00".repeat(31))).is_none());
    }

    #[test]
    fn statement_signature_recovers_to_the_signer_and_binds_nonce_and_quote() {
        let key = SigningKey::from_slice(&KEY).unwrap();
        let nonce = [9u8; 32];
        let quote = b"quote-bytes";
        let sig = sign_statement(&key, &nonce, quote).unwrap();
        assert_eq!(sig.len(), 65);
        let signature = Signature::from_slice(&sig[..64]).unwrap();
        let recid = RecoveryId::from_byte(sig[64]).unwrap();
        let recovered =
            VerifyingKey::recover_from_digest(statement_digest(&nonce, quote), &signature, recid)
                .unwrap();
        assert_eq!(&recovered, key.verifying_key());
        // A different nonce or quote yields a different digest.
        let other = VerifyingKey::recover_from_digest(
            statement_digest(&[8u8; 32], quote),
            &signature,
            recid,
        );
        assert!(other.map(|k| &k != key.verifying_key()).unwrap_or(true));
        let other = VerifyingKey::recover_from_digest(
            statement_digest(&nonce, b"other-quote"),
            &signature,
            recid,
        );
        assert!(other.map(|k| &k != key.verifying_key()).unwrap_or(true));
    }

    /// Cross-language vector: lit-agent-keychain/tests/attestation-v2.test.ts
    /// verifies this exact signature with the SDK's verifier.
    #[test]
    fn statement_vector_matches_the_sdk_fixture() {
        let key = SigningKey::from_slice(&KEY).unwrap();
        let nonce = [0x11u8; 32];
        let quote = hex::decode("0400020081").unwrap();
        let sig = sign_statement(&key, &nonce, &quote).unwrap();
        assert_eq!(
            hex::encode(key.verifying_key().to_sec1_bytes()),
            "02989c0b76cb563971fdc9bef31ec06c3560f3249d6ee9e5d83c57625596e05f6f"
        );
        assert_eq!(hex::encode(&sig), STATEMENT_VECTOR_SIGNATURE);
    }
    const STATEMENT_VECTOR_SIGNATURE: &str = "4a842a94c2a19f06f4056745c42d82046a0048a908b5c779fd377e2eb547cf741b900a4a62e68ff9ce45db9640d4528382a816fdf53a78f62daee473337840df00";

    fn recover(sig_hex: &str, digest: Keccak256) -> VerifyingKey {
        let sig = hex::decode(sig_hex).unwrap();
        assert_eq!(sig.len(), 65);
        let signature = Signature::from_slice(&sig[..64]).unwrap();
        let recid = RecoveryId::from_byte(sig[64]).unwrap();
        VerifyingKey::recover_from_digest(digest, &signature, recid).unwrap()
    }

    /// KMS root keys (compressed secp256k1) a signer chain may end at: Phala's
    /// KMS on Base (`DstackKms.kmsInfo()` at 0x2f83…ba9c, also pinned in the
    /// SDK) and the dstack simulator's built-in KMS (`sdk/simulator/appkeys.json`).
    const KNOWN_KMS_ROOTS: [&str; 2] = [
        "02a335aea3df11d46ae0a373d3f26fbd1d208572ecc87e630e618dac693b7af721",
        "0321529e458424ab1f710a3a57ec4dad2fb195ddca572f7469242ba6c7563085b6",
    ];

    /// Against a real dstack agent (TEE or the phala-simulator workflow's
    /// simulator): the signer's chain verifies in exactly the format the SDK
    /// checks, ending at a known KMS root, and the statement covers nonce and
    /// quote. No-ops when no dstack socket is present.
    #[tokio::test]
    async fn signer_chain_verifies_against_a_live_dstack_agent() {
        if !dstack::socket_available(&dstack::resolve_socket_path()) {
            return;
        }
        let info = dstack::get_info().await.expect("dstack info");
        let app_id = hex::decode(info.app_id.trim_start_matches("0x")).unwrap();
        let signer = signer().await.expect("attestation signer");
        let app_key = recover(
            &signer.info.app_signature,
            Keccak256::new_with_prefix(format!(
                "{}:{}",
                signer.info.purpose, signer.info.public_key
            )),
        );
        let mut kms_message = b"dstack-kms-issued:".to_vec();
        kms_message.extend_from_slice(&app_id);
        kms_message.extend_from_slice(&app_key.to_sec1_bytes());
        let kms_key = recover(
            &signer.info.kms_signature,
            Keccak256::new_with_prefix(kms_message),
        );
        // Recovery yields some key for any input; only a known root proves the
        // chain (and the message format) is the KMS's.
        let kms_root = hex::encode(kms_key.to_sec1_bytes());
        assert!(
            KNOWN_KMS_ROOTS.contains(&kms_root.as_str()),
            "signer chain ends at unknown KMS root {kms_root}"
        );

        let quote = cached_quote().await.expect("quote");
        let nonce = [0x5au8; 32];
        let sig = sign_statement(&signer.key, &nonce, &quote.quote_bytes).unwrap();
        assert_eq!(
            recover(
                &hex::encode(sig),
                statement_digest(&nonce, &quote.quote_bytes)
            ),
            *signer.key.verifying_key()
        );
        // Cached: a second request reuses the same quote.
        assert!(Arc::ptr_eq(&quote, &cached_quote().await.unwrap()));
    }

    #[test]
    fn signer_requires_a_secp256k1_key_and_two_link_chain() {
        let chain = vec![vec![1u8; 65], vec![2u8; 65]];
        let signer = signer_from_key_response(&KEY, &chain).unwrap();
        assert_eq!(signer.info.purpose, SIGNER_PURPOSE);
        assert_eq!(signer.info.public_key.len(), 66);
        assert_eq!(signer.info.app_signature, hex::encode([1u8; 65]));
        assert_eq!(signer.info.kms_signature, hex::encode([2u8; 65]));
        assert!(signer_from_key_response(&KEY, &chain[..1]).is_err());
        assert!(signer_from_key_response(&KEY, &[vec![1u8; 64], vec![2u8; 65]]).is_err());
        assert!(signer_from_key_response(&[0u8; 32], &chain).is_err());
    }
}
