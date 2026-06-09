//! EIP-712 typed-data verification for ChainSecured signatures (CPL-286).
//!
//! Wallet UIs render the signed payload as a typed struct with `primaryType`
//! clearly labeled. Cross-flow replay is rejected at the type-hash level:
//! a signature minted against `CreateWallet` cannot recover when the server
//! hashes the same bytes against, say, `BillingAuth` or `AddUsageApiKey` —
//! the type hash commits to the struct name, so different `primaryType`
//! values produce different EIP-712 digests even with identical field
//! values.
//!
//! Server-authoritative model: the client posts the full EIP-712 typed data
//! the wallet signed. The server validates the domain, the schema for the
//! claimed `primaryType`, and the timestamp window before doing any
//! crypto — then recovers the signer from the digest and confirms it
//! matches the `address` field inside the typed data.
//!
//! The wallet UI determines the digest. If the client builds different
//! bytes than the server expects, the digest differs and recovery fails —
//! no out-of-band type-hash check needed beyond pinning the schema.

use std::collections::BTreeMap;

use alloy::dyn_abi::TypedData;
use alloy::primitives::{Address, B256, Bytes, FixedBytes, Signature, U256};
use alloy::sol;

use crate::config::GLOBAL_NODE_CONFIG;
use crate::core::v1::helpers::api_status::ApiStatus;

/// ERC-1271 `isValidSignature(bytes32,bytes)` magic return value
/// (`bytes4(keccak256("isValidSignature(bytes32,bytes)"))`). A smart-contract
/// wallet (Safe, etc.) returns exactly these four bytes when it considers the
/// signature valid for the given hash.
const ERC1271_MAGIC_VALUE: [u8; 4] = [0x16, 0x26, 0xba, 0x7e];

sol! {
    #[sol(rpc)]
    interface IERC1271 {
        function isValidSignature(bytes32 hash, bytes signature) external view returns (bytes4);
    }
}

/// Hard cap on the decoded signature length. The EOA path needs exactly 65
/// bytes; EIP-1271 contract signatures are larger (a Safe is ~65 bytes per
/// owner plus overhead), but never anywhere near this. Rocket permits multi-MiB
/// JSON bodies, and these mint endpoints are unauthenticated — without this cap
/// an attacker could ship a multi-MiB hex blob to force decode/allocation and
/// then ABI-encode it into giant `eth_call` calldata. 4 KiB covers ~60 Safe
/// owners; legitimate payloads are far smaller.
pub(crate) const MAX_SIGNATURE_BYTES: usize = 4096;

/// Gas ceiling for the EIP-1271 `isValidSignature` `eth_call`. Signature
/// verification (even a large Safe) costs well under this; the cap bounds the
/// node/RPC CPU a malicious contract at an attacker-chosen address can burn per
/// unauthenticated request.
const ERC1271_CALL_GAS_LIMIT: u64 = 1_000_000;

/// Wall-clock timeout for the EIP-1271 `eth_call`. Bounds how long a slow or
/// adversarial RPC / contract can tie up a request handler.
const ERC1271_CALL_TIMEOUT_SECS: u64 = 5;

/// ±5-minute window on `issuedAt`; the only replay protection (no nonce
/// store). Worst-case replay on the unauthenticated mint endpoints just
/// produces an extra unattached PKP — the bytes returned are equivalent to
/// a freshly generated keypair until the admin wallet calls the on-chain
/// follow-ups, so compute cost only.
pub(crate) const TIMESTAMP_SKEW_SECONDS: i64 = 300;

/// Hard cap on the JSON-serialised typed-data payload to bound the work an
/// unauthenticated caller can force the server to do before any cheap
/// reject. 4 KiB is far above any legitimate payload — the canonical shape
/// is well under 1 KiB.
pub(crate) const MAX_TYPED_DATA_LEN: usize = 4096;

/// Stable EIP-712 domain `name` for all ChainSecured signatures. Wallet UIs
/// surface this; do not change without a coordinated client release.
pub(crate) const EIP712_DOMAIN_NAME: &str = "Lit ChainSecured";

/// Domain `version`. Bump only when the typed-data schema changes in a way
/// that requires re-signing — wallets refuse to interpret a v2 signature
/// against a v1 domain (and vice versa).
pub(crate) const EIP712_DOMAIN_VERSION: &str = "1";

/// Canonical primary types per ChainSecured flow. Each endpoint pins one
/// of these and rejects signatures whose `primaryType` doesn't match — the
/// type hash commits to the struct name, so a signature minted for one
/// flow cannot recover when the server hashes the same bytes against a
/// different `primaryType`.
pub(crate) const PRIMARY_TYPE_CREATE_WALLET: &str = "CreateWallet";
pub(crate) const PRIMARY_TYPE_CONVERT_ACCOUNT: &str = "ConvertAccount";
pub(crate) const PRIMARY_TYPE_ADD_USAGE_API_KEY: &str = "AddUsageApiKey";
pub(crate) const PRIMARY_TYPE_BILLING_AUTH: &str = "BillingAuth";

/// One field of an EIP-712 type declaration. Mirrors the wire shape
/// (`{"name": "...", "type": "..."}`) so we can validate the client-supplied
/// `types` map byte-for-byte against the canonical schema. We don't use
/// alloy's `PropertyDef` directly here because comparison ergonomics with
/// string literals are simpler with a plain struct, and the JSON shape we
/// validate against is fixed.
///
/// `deny_unknown_fields` is load-bearing: a phishing dApp could otherwise
/// embed a decoy field like `{ "name": "address", "type": "address",
/// "label": "Approve $500" }`. The extra key doesn't change the EIP-712
/// type hash (only `name` + `type` are encoded), so the digest still
/// recovers, but some wallet UIs surface the unknown metadata to the
/// user. The schema-equality check below sees only the canonical fields
/// and would silently pass without this guard.
#[derive(Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Eip712FieldDef {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

impl Eip712FieldDef {
    fn new(name: &str, ty: &str) -> Self {
        Self {
            name: name.to_string(),
            ty: ty.to_string(),
        }
    }
}

/// Just the slice of the JSON we need to validate the schema. Alloy's
/// `TypedData::resolver` is opaque (private fields), so we deserialize the
/// `types` and `primaryType` keys into our own view and run schema checks
/// against that. The same JSON is also fed to `alloy::dyn_abi::TypedData`
/// to compute the EIP-712 digest — serde_json yields a deterministic logical
/// value, so both parses see the same fields.
/// `deny_unknown_fields` on the top-level view: a phishing payload could
/// otherwise smuggle in an extra top-level key (e.g. `displayMessage`) that
/// some wallet UIs surface alongside the signed struct. The four canonical
/// keys are exhaustive for what this server validates.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedDataSchemaView {
    types: BTreeMap<String, Vec<Eip712FieldDef>>,
    #[serde(rename = "primaryType")]
    primary_type: String,
    domain: DomainView,
    message: BTreeMap<String, serde_json::Value>,
}

/// `deny_unknown_fields` is load-bearing on the domain too — the goal is
/// strict anti-phishing pinning to exactly `(name, version, chainId)`. The
/// explicit `verifying_contract`/`salt` rejects below still trip if those
/// fields are present (so the error message stays specific for the common
/// case), but anything *else* (e.g. `displayName`) is rejected here.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainView {
    name: Option<String>,
    version: Option<String>,
    #[serde(rename = "chainId")]
    chain_id: Option<serde_json::Value>,
    #[serde(default, rename = "verifyingContract")]
    verifying_contract: Option<serde_json::Value>,
    #[serde(default)]
    salt: Option<serde_json::Value>,
}

/// The four flows share an identical message struct: `(address, issuedAt)`.
/// Field declaration order is part of the EIP-712 type hash — clients must
/// declare the fields in this order, and any reordering is rejected by
/// `validate_type_schema`.
fn payload_field_schema() -> [Eip712FieldDef; 2] {
    [
        Eip712FieldDef::new("address", "address"),
        Eip712FieldDef::new("issuedAt", "uint256"),
    ]
}

/// Canonical `EIP712Domain` field declaration. We use the (name, version,
/// chainId) subset — no `verifyingContract` (this server is not an
/// on-chain contract) and no `salt`. Clients must declare these in the
/// same order; any other shape is rejected.
fn domain_field_schema() -> [Eip712FieldDef; 3] {
    [
        Eip712FieldDef::new("name", "string"),
        Eip712FieldDef::new("version", "string"),
        Eip712FieldDef::new("chainId", "uint256"),
    ]
}

fn is_known_primary_type(s: &str) -> bool {
    matches!(
        s,
        PRIMARY_TYPE_CREATE_WALLET
            | PRIMARY_TYPE_CONVERT_ACCOUNT
            | PRIMARY_TYPE_ADD_USAGE_API_KEY
            | PRIMARY_TYPE_BILLING_AUTH
    )
}

/// Build the canonical typed data JSON the server expects for a given flow.
/// Used by tests to round-trip a wallet signature; not part of the request
/// path (the request path validates the client-supplied typed data instead
/// of rebuilding it, so the wallet's view and the server's view are bit-
/// identical when verification succeeds).
#[cfg(test)]
pub(crate) fn build_canonical_typed_data_json(
    primary_type: &str,
    address: Address,
    issued_at: i64,
    chain_id: u64,
) -> serde_json::Value {
    let payload_fields: Vec<serde_json::Value> = payload_field_schema()
        .iter()
        .map(|f| serde_json::json!({ "name": f.name, "type": f.ty }))
        .collect();
    let domain_fields: Vec<serde_json::Value> = domain_field_schema()
        .iter()
        .map(|f| serde_json::json!({ "name": f.name, "type": f.ty }))
        .collect();

    serde_json::json!({
        "types": {
            "EIP712Domain": domain_fields,
            primary_type: payload_fields,
        },
        "primaryType": primary_type,
        "domain": {
            "name": EIP712_DOMAIN_NAME,
            "version": EIP712_DOMAIN_VERSION,
            // chainId is stringified to match the wire shape JS wallets use.
            "chainId": chain_id.to_string(),
        },
        "message": {
            "address": format!("{:#x}", address),
            "issuedAt": issued_at.to_string(),
        },
    })
}

/// Outcome of the cheap, synchronous portion of verification: everything
/// validated and parsed up to (but not including) the address-equality check.
/// `ecdsa_recovered` is `Some` when the signature is a well-formed 65-byte
/// ECDSA signature that recovers to a concrete address (the EOA path), and
/// `None` for contract-wallet signatures (wrong length, or recovery failed) —
/// those are resolved on-chain via EIP-1271 instead.
struct PreparedVerification {
    /// The address claimed in `message.address`.
    claimed_address: Address,
    /// The EIP-712 signing hash the wallet signed.
    digest: B256,
    /// Raw signature bytes, as posted. Passed verbatim to EIP-1271
    /// `isValidSignature` for the contract-wallet path.
    signature: Vec<u8>,
    /// Address recovered via ECDSA, if the signature is a standard 65-byte EOA
    /// signature. `None` for smart-contract-wallet signatures.
    ecdsa_recovered: Option<Address>,
}

/// Verify an EIP-712 typed-data + signature pair for a specific ChainSecured
/// flow against an **EOA only** (standard ECDSA `ecrecover`). Returns the
/// recovered wallet address on success.
///
/// Smart-contract wallets (Safe, etc.) have no private key and produce
/// EIP-1271 signatures that do not recover to the wallet address — use
/// [`verify_eip712_signature_allow_contract_wallet`] for flows that must
/// accept those.
///
/// Cheap rejects (length cap, JSON parse, schema match, domain match,
/// primary-type match, timestamp window) run before the expensive ECDSA
/// recovery so junk traffic with stale, wrong-purpose, or wrong-chain
/// payloads is dropped without doing crypto.
pub(crate) fn verify_eip712_signature(
    typed_data_json: &serde_json::Value,
    signature_hex: &str,
    expected_primary_type: &str,
) -> Result<Address, ApiStatus> {
    let prepared = prepare_verification(typed_data_json, signature_hex, expected_primary_type)?;
    if prepared.ecdsa_recovered == Some(prepared.claimed_address) {
        return Ok(prepared.claimed_address);
    }
    Err(ApiStatus::bad_request(
        anyhow::anyhow!("Signature does not match claimed address"),
        "Signature does not match claimed address",
    ))
}

/// Like [`verify_eip712_signature`], but additionally accepts EIP-1271
/// smart-contract-wallet signatures (e.g. a Gnosis Safe). When standard ECDSA
/// recovery does not match the claimed address, the claimed address is treated
/// as a contract and `isValidSignature(digest, signature)` is called on-chain
/// (against the node's configured chain — the same chain pinned in the EIP-712
/// domain). Verification succeeds iff the contract returns the ERC-1271 magic
/// value.
///
/// Counterfactual (not-yet-deployed) wallets are not supported — ERC-6492 is
/// out of scope; the contract must already be deployed on-chain.
pub(crate) async fn verify_eip712_signature_allow_contract_wallet(
    typed_data_json: &serde_json::Value,
    signature_hex: &str,
    expected_primary_type: &str,
) -> Result<Address, ApiStatus> {
    let prepared = prepare_verification(typed_data_json, signature_hex, expected_primary_type)?;
    if prepared.ecdsa_recovered == Some(prepared.claimed_address) {
        return Ok(prepared.claimed_address);
    }
    verify_erc1271_signature(
        prepared.claimed_address,
        prepared.digest,
        &prepared.signature,
    )
    .await?;
    Ok(prepared.claimed_address)
}

/// Run the synchronous validation pipeline and parse the signature. Shared by
/// both the EOA-only and contract-wallet-aware entry points.
fn prepare_verification(
    typed_data_json: &serde_json::Value,
    signature_hex: &str,
    expected_primary_type: &str,
) -> Result<PreparedVerification, ApiStatus> {
    // Length cap before parsing — bound the work an unauthenticated caller
    // can force the server to do before we hit anything expensive.
    let serialized = serde_json::to_string(typed_data_json)
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "typed_data not serializable"))?;
    if serialized.len() > MAX_TYPED_DATA_LEN {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("typed_data exceeds {}-byte cap", MAX_TYPED_DATA_LEN),
            "typed_data too large",
        ));
    }

    // Accept both wire shapes for the typed-data payload: the object form
    // (what viem and ethers.js v6 send) and the stringified form (what
    // `eth_signTypedData_v4` returns from MetaMask, which some clients
    // forward as-is). Both ethers' and alloy's `TypedData` deserializers
    // accept both shapes, so the prior verifier handled both; rejecting
    // the stringified form would silently break those clients.
    let typed_data_json = normalize_typed_data_input(typed_data_json)?;

    let view: TypedDataSchemaView =
        serde_json::from_value(typed_data_json.clone()).map_err(|e| {
            ApiStatus::bad_request(
                anyhow::anyhow!(e),
                "typed_data is not a valid EIP-712 typed-data object",
            )
        })?;

    if view.primary_type != expected_primary_type {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "primaryType mismatch: typed_data says {:?}, this endpoint expects {:?}",
                view.primary_type,
                expected_primary_type
            ),
            "primaryType mismatch — signature was minted for a different flow",
        ));
    }
    if !is_known_primary_type(&view.primary_type) {
        // Defence in depth — `expected_primary_type` is always one of our
        // constants, but pin the wider invariant explicitly so adding a new
        // primary type forces us to update `is_known_primary_type` too.
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Unknown primaryType {:?}", view.primary_type),
            "Unknown primaryType",
        ));
    }

    validate_domain(&view.domain)?;
    validate_type_schema(&view)?;
    let address = extract_address(&view)?;
    let issued_at = extract_issued_at(&view)?;
    validate_timestamp(issued_at)?;

    // Parse a second time into alloy's TypedData to compute the EIP-712
    // digest. Two parses of the same JSON yield the same logical value.
    let typed_data: TypedData = serde_json::from_value(typed_data_json).map_err(|e| {
        ApiStatus::bad_request(
            anyhow::anyhow!(e),
            "typed_data is not a valid EIP-712 typed-data object",
        )
    })?;
    let digest = typed_data.eip712_signing_hash().map_err(|e| {
        ApiStatus::internal_server_error(
            anyhow::anyhow!("eip712_signing_hash failed: {}", e),
            "eip712_signing_hash failed",
        )
    })?;
    // Decode the raw signature bytes. We keep these verbatim for the EIP-1271
    // contract path (where the signature can be arbitrary-length, not a 65-byte
    // ECDSA tuple) and additionally attempt standard ECDSA recovery for the EOA
    // path.
    let sig_str = signature_hex.trim();
    // Cap the hex length before decoding so an oversized blob is rejected
    // without allocating the decoded buffer (hex is 2 chars/byte, plus the
    // optional "0x").
    if sig_str.len() > MAX_SIGNATURE_BYTES * 2 + 2 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("signature exceeds {}-byte cap", MAX_SIGNATURE_BYTES),
            "Signature too large",
        ));
    }
    let sig_bytes = hex::decode(sig_str.strip_prefix("0x").unwrap_or(sig_str))
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "Invalid signature hex"))?;
    if sig_bytes.len() > MAX_SIGNATURE_BYTES {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("signature exceeds {}-byte cap", MAX_SIGNATURE_BYTES),
            "Signature too large",
        ));
    }

    // ECDSA recovery only applies to a standard 65-byte signature. Anything
    // else is a contract-wallet (EIP-1271) signature, resolved on-chain by the
    // caller. A 65-byte blob that fails to parse/recover also falls through to
    // the contract path rather than erroring here.
    let ecdsa_recovered = if sig_bytes.len() == 65 {
        Signature::try_from(sig_bytes.as_slice())
            .ok()
            .and_then(|sig| sig.recover_address_from_prehash(&digest).ok())
    } else {
        None
    };

    Ok(PreparedVerification {
        claimed_address: address,
        digest,
        signature: sig_bytes,
        ecdsa_recovered,
    })
}

/// Verify a signature via EIP-1271 (`isValidSignature`) against an already-
/// deployed smart-contract wallet at `address`, on the node's configured chain.
async fn verify_erc1271_signature(
    address: Address,
    digest: B256,
    signature: &[u8],
) -> Result<(), ApiStatus> {
    let provider = crate::accounts::signable_contract::get_read_only_client().map_err(|e| {
        ApiStatus::internal_server_error(e, "Chain client unavailable for EIP-1271 verification")
    })?;

    let contract = IERC1271::new(address, provider);
    let call = contract
        .isValidSignature(digest, Bytes::from(signature.to_vec()))
        .gas(ERC1271_CALL_GAS_LIMIT);

    // Bound the call in wall-clock time (slow/adversarial RPC or contract) on
    // top of the gas cap above.
    let magic = match tokio::time::timeout(
        std::time::Duration::from_secs(ERC1271_CALL_TIMEOUT_SECS),
        call.call(),
    )
    .await
    {
        Err(_elapsed) => {
            // Keep RPC/timing detail server-side; do not leak it to the client.
            tracing::warn!(
                "EIP-1271 isValidSignature timed out after {}s for {address}",
                ERC1271_CALL_TIMEOUT_SECS
            );
            return Err(ApiStatus::bad_request(
                anyhow::anyhow!("EIP-1271 verification timed out"),
                "Signature does not match claimed address",
            ));
        }
        Ok(Ok(magic)) => magic,
        Ok(Err(e)) => {
            // An EOA (no contract code) or a contract that doesn't implement
            // EIP-1271 returns no decodable `bytes4`, surfacing here. Log the
            // RPC/contract detail server-side and return a generic message so
            // revert strings and RPC internals don't leak to the client.
            tracing::warn!("EIP-1271 isValidSignature call failed for {address}: {e}");
            return Err(ApiStatus::bad_request(
                anyhow::anyhow!("EIP-1271 verification failed"),
                "Signature does not match claimed address",
            ));
        }
    };

    if magic == FixedBytes::<4>::from(ERC1271_MAGIC_VALUE) {
        Ok(())
    } else {
        Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "EIP-1271 isValidSignature returned non-magic value {:#x}",
                magic
            ),
            "Signature does not match claimed address",
        ))
    }
}

fn validate_domain(domain: &DomainView) -> Result<(), ApiStatus> {
    if domain.name.as_deref() != Some(EIP712_DOMAIN_NAME) {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Domain name mismatch: typed_data says {:?}, expected {:?}",
                domain.name,
                EIP712_DOMAIN_NAME
            ),
            "Domain name mismatch",
        ));
    }
    if domain.version.as_deref() != Some(EIP712_DOMAIN_VERSION) {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Domain version mismatch: typed_data says {:?}, expected {:?}",
                domain.version,
                EIP712_DOMAIN_VERSION
            ),
            "Domain version mismatch",
        ));
    }
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))
        .map_err(|e| ApiStatus::internal_server_error(e, "GLOBAL_NODE_CONFIG missing"))?;
    let expected_chain_id = U256::from(node_config.chain.info().chain_id);
    let actual_chain_id = domain
        .chain_id
        .as_ref()
        .map(|v| parse_u256_loose(v, "chainId"))
        .transpose()
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "Invalid chainId in domain"))?;
    if actual_chain_id != Some(expected_chain_id) {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Chain ID mismatch: typed_data says {:?}, server is on {}",
                domain.chain_id,
                expected_chain_id
            ),
            "Chain ID mismatch",
        ));
    }
    // Reject extra domain fields. A phishing site could otherwise include
    // a verifyingContract or salt the user doesn't notice and bind the same
    // bytes to a different domain than the wallet displays. We strictly
    // pin the domain to (name, version, chainId).
    if domain.verifying_contract.is_some() || domain.salt.is_some() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Unexpected domain fields"),
            "Domain must contain only name, version, chainId",
        ));
    }
    Ok(())
}

/// Parse a `uint256` JSON value (chainId, issuedAt, …) as either a numeric
/// string ("175188"), a hex string ("0x2ac14"), or a JSON number. Matches
/// what JS wallets, viem, and ethers.js produce across versions; the
/// EIP-712 spec doesn't pin a single wire shape for uint256.
fn parse_u256_loose(v: &serde_json::Value, field: &str) -> Result<U256, anyhow::Error> {
    match v {
        serde_json::Value::String(s) => {
            let s = s.trim();
            if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                U256::from_str_radix(rest, 16)
                    .map_err(|e| anyhow::anyhow!("invalid hex {field}: {e}"))
            } else {
                s.parse::<U256>()
                    .map_err(|e| anyhow::anyhow!("invalid decimal {field}: {e}"))
            }
        }
        serde_json::Value::Number(n) => {
            let u = n
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("{field} must be a non-negative integer"))?;
            Ok(U256::from(u))
        }
        _ => Err(anyhow::anyhow!("{field} must be a string or number")),
    }
}

/// Accept both wire shapes for the typed-data payload:
///   1. The object form: `req.typed_data = { types: {...}, ... }`
///   2. The stringified form: `req.typed_data = "{ \"types\": ... }"`
///
/// `eth_signTypedData_v4` returns a JSON string; some clients forward it
/// verbatim as the `typed_data` field. Both ethers' `TypedData` and
/// alloy's `TypedData` deserializers accept both shapes, so the prior
/// verifier silently handled both and we preserve that here.
fn normalize_typed_data_input(
    typed_data_json: &serde_json::Value,
) -> Result<serde_json::Value, ApiStatus> {
    match typed_data_json {
        serde_json::Value::String(s) => serde_json::from_str(s).map_err(|e| {
            ApiStatus::bad_request(
                anyhow::anyhow!(e),
                "typed_data is a string but not valid JSON",
            )
        }),
        other => Ok(other.clone()),
    }
}

fn validate_type_schema(view: &TypedDataSchemaView) -> Result<(), ApiStatus> {
    let expected_payload = payload_field_schema();
    let primary_decl = view.types.get(&view.primary_type).ok_or_else(|| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Missing types[{}]", view.primary_type),
            "Missing primaryType in types",
        )
    })?;
    if primary_decl.as_slice() != expected_payload.as_slice() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "primaryType field schema mismatch — expected {:?}",
                expected_payload
            ),
            "primaryType field schema mismatch",
        ));
    }

    let expected_domain = domain_field_schema();
    let domain_decl = view.types.get("EIP712Domain").ok_or_else(|| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Missing types[EIP712Domain]"),
            "Missing EIP712Domain in types",
        )
    })?;
    if domain_decl.as_slice() != expected_domain.as_slice() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "EIP712Domain field schema mismatch — expected {:?}",
                expected_domain
            ),
            "EIP712Domain field schema mismatch",
        ));
    }

    // Reject any extra type definitions the client tried to smuggle in.
    // We don't use them, but a wallet might display them (depending on
    // implementation) and the user could be misled into thinking they
    // signed something different.
    if view.types.len() != 2 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Unexpected type definitions — expected only EIP712Domain and {}",
                view.primary_type
            ),
            "Unexpected type definitions",
        ));
    }

    // Strict check on `message` content. The EIP-712 type hash only commits
    // to the fields declared in `types[primaryType]`, so extras in `message`
    // don't change the digest — but a phishing dApp could still embed
    // decoy fields (e.g. `intent: "Subscribe to weekly briefing"`) that
    // some wallet UIs render alongside the canonical fields, fooling the
    // user into signing what looks like a different action. Pin the
    // message to exactly the canonical (address, issuedAt) pair.
    if view.message.len() != 2
        || !view.message.contains_key("address")
        || !view.message.contains_key("issuedAt")
    {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "message must contain exactly `address` and `issuedAt`, got {:?}",
                view.message.keys().collect::<Vec<_>>()
            ),
            "message contains unexpected fields",
        ));
    }
    Ok(())
}

fn extract_address(view: &TypedDataSchemaView) -> Result<Address, ApiStatus> {
    let address_val = view.message.get("address").ok_or_else(|| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Missing message.address"),
            "Missing message.address",
        )
    })?;
    let address: Address = serde_json::from_value(address_val.clone())
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "Invalid message.address"))?;
    Ok(address)
}

fn extract_issued_at(view: &TypedDataSchemaView) -> Result<i64, ApiStatus> {
    let issued_val = view.message.get("issuedAt").ok_or_else(|| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Missing message.issuedAt"),
            "Missing message.issuedAt",
        )
    })?;
    // The field is declared `uint256` in the schema. JS wallets generally
    // emit it as a decimal string, but alloy and ethers both accept hex
    // strings and JSON numbers for uint256 — and the digest the wallet
    // signed is computed from whichever wire shape they used. Mirror that
    // here so we don't reject pre-recovery on a payload that would hash
    // correctly. Then bound to i64::MAX so the ±skew arithmetic below
    // can't overflow.
    let n = parse_u256_loose(issued_val, "issuedAt")
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "issuedAt not a uint256 value"))?;
    let i64_max_as_u256 = U256::from(i64::MAX as u64);
    if n > i64_max_as_u256 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("issuedAt exceeds i64::MAX ({})", i64::MAX),
            "issuedAt out of range",
        ));
    }
    // Safe: bounded to i64::MAX above.
    let as_u64: u64 = n.to::<u64>();
    Ok(as_u64 as i64)
}

fn validate_timestamp(issued_at: i64) -> Result<(), ApiStatus> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!(e),
                "System clock is before the Unix epoch",
            )
        })?
        .as_secs() as i64;
    // i128 subtraction: two i64s can never overflow into i128, and i128::abs
    // is safe for any value produced by an i64 subtraction. Without this, a
    // crafted issuedAt of i64::MIN would wrap `(now - issued_at).abs()` back
    // to a small/negative value and bypass the skew check in release builds.
    let delta = (now as i128) - (issued_at as i128);
    if delta.abs() > TIMESTAMP_SKEW_SECONDS as i128 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Issued At {} is outside the ±{}s window from now ({})",
                issued_at,
                TIMESTAMP_SKEW_SECONDS,
                now
            ),
            "Signed message timestamp is too old or too far in the future",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::dyn_abi::TypedData;
    use alloy::signers::SignerSync;
    use alloy::signers::local::PrivateKeySigner;

    /// `GLOBAL_NODE_CONFIG` is a `OnceLock` populated at server startup. Tests
    /// share it — only the first `get_or_init` wins, so every test below sees
    /// the same chain. We pin to `Anvil` (chain_id 175188) since that's the
    /// fixture chain used elsewhere in the test suite and matches what a CI
    /// run sees.
    fn ensure_test_chain_id() -> u64 {
        let nc = crate::config::GLOBAL_NODE_CONFIG.get_or_init(|| crate::config::NodeConfig {
            chain: crate::utils::chain_info::Chain::Anvil,
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
        });
        nc.chain.info().chain_id
    }

    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Builds canonical typed data, signs it with `wallet`, returns
    /// `(typed_data_json, signature_hex)` so tests can hand them to the
    /// verifier in the same shape an HTTP request would.
    fn sign_canonical(
        wallet: &PrivateKeySigner,
        primary_type: &str,
        issued_at: i64,
        chain_id: u64,
    ) -> (serde_json::Value, String) {
        let json =
            build_canonical_typed_data_json(primary_type, wallet.address(), issued_at, chain_id);
        let typed_data: TypedData = serde_json::from_value(json.clone()).unwrap();
        let digest = typed_data.eip712_signing_hash().unwrap();
        let sig = wallet.sign_hash_sync(&digest).unwrap();
        (json, format!("0x{}", hex::encode(sig.as_bytes())))
    }

    #[test]
    fn happy_path_create_wallet() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        let recovered = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET).unwrap();
        assert_eq!(recovered, wallet.address());
    }

    #[test]
    fn happy_path_all_primary_types() {
        let chain_id = ensure_test_chain_id();
        for primary in [
            PRIMARY_TYPE_CREATE_WALLET,
            PRIMARY_TYPE_CONVERT_ACCOUNT,
            PRIMARY_TYPE_ADD_USAGE_API_KEY,
            PRIMARY_TYPE_BILLING_AUTH,
        ] {
            let wallet = PrivateKeySigner::random();
            let (typed, sig) = sign_canonical(&wallet, primary, now_secs(), chain_id);
            let recovered = verify_eip712_signature(&typed, &sig, primary)
                .unwrap_or_else(|e| panic!("expected {primary} to verify, got: {e:?}"));
            assert_eq!(recovered, wallet.address(), "{primary} address mismatch");
        }
    }

    /// The core CPL-286 promise: a signature minted for one flow must not
    /// recover when the server hashes the typed data against a different
    /// primaryType. This is rejected at the schema layer (we check
    /// `expected_primary_type` before recovery), but the deeper guarantee
    /// is that even if the schema check were bypassed, the digest would
    /// differ. Both layers are exercised here.
    #[test]
    fn cross_flow_replay_rejected_at_primary_type_check() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        // User signs a CreateWallet typed payload.
        let (typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        // Attacker forwards it to the AddUsageApiKey endpoint as-is.
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_ADD_USAGE_API_KEY)
            .expect_err("must reject — primaryType mismatch");
        let msg = format!("{err}");
        assert!(
            msg.contains("primaryType") || msg.contains("primary_type"),
            "unexpected error: {msg}",
        );
    }

    /// Even if the attacker rewrites the `primaryType` field after the fact
    /// (so it matches the target endpoint), the signature will not recover
    /// because the EIP-712 digest commits to the type hash.
    #[test]
    fn cross_flow_replay_rejected_at_recovery_when_primary_type_rewritten() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        // User signs CreateWallet.
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        // Attacker rewrites primaryType + types map to look like AddUsageApiKey.
        typed["primaryType"] = serde_json::json!(PRIMARY_TYPE_ADD_USAGE_API_KEY);
        let payload_fields = serde_json::json!([
            { "name": "address", "type": "address" },
            { "name": "issuedAt", "type": "uint256" },
        ]);
        typed["types"]
            .as_object_mut()
            .unwrap()
            .remove(PRIMARY_TYPE_CREATE_WALLET);
        typed["types"]
            .as_object_mut()
            .unwrap()
            .insert(PRIMARY_TYPE_ADD_USAGE_API_KEY.to_string(), payload_fields);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_ADD_USAGE_API_KEY)
            .expect_err("must reject — digest doesn't match the rewritten type hash");
        let msg = format!("{err}");
        assert!(
            msg.contains("Signature does not match") || msg.contains("Signature recovery failed"),
            "unexpected error: {msg}",
        );
    }

    #[test]
    fn rejects_chain_id_mismatch() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (typed, sig) = sign_canonical(
            &wallet,
            PRIMARY_TYPE_CREATE_WALLET,
            now_secs(),
            chain_id.wrapping_add(1),
        );
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — wrong chainId");
        assert!(format!("{err}").contains("Chain ID"));
    }

    #[test]
    fn rejects_domain_name_mismatch() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        typed["domain"]["name"] = serde_json::json!("Not Lit ChainSecured");
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — wrong domain name");
        assert!(format!("{err}").contains("Domain name"));
    }

    #[test]
    fn rejects_domain_version_mismatch() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        typed["domain"]["version"] = serde_json::json!("99");
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — wrong domain version");
        assert!(format!("{err}").contains("Domain version"));
    }

    #[test]
    fn rejects_extra_domain_field_verifying_contract() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        typed["domain"]["verifyingContract"] =
            serde_json::json!("0x1111111111111111111111111111111111111111");
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — verifyingContract is not allowed");
        assert!(format!("{err}").contains("Domain"));
    }

    #[test]
    fn rejects_schema_field_reorder() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        // Swap field order on the primary type — produces a different type hash.
        typed["types"][PRIMARY_TYPE_CREATE_WALLET] = serde_json::json!([
            { "name": "issuedAt", "type": "uint256" },
            { "name": "address", "type": "address" },
        ]);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — schema fields reordered");
        assert!(format!("{err}").contains("schema"));
    }

    #[test]
    fn rejects_extra_type_definitions() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        // Smuggle in an extra type the wallet UI might display alongside.
        typed["types"]["TotallyLegit"] = serde_json::json!([
            { "name": "spookyField", "type": "string" },
        ]);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — types must contain exactly EIP712Domain + primaryType");
        assert!(format!("{err}").contains("Unexpected type definitions"));
    }

    #[test]
    fn rejects_timestamp_too_old() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let stale = now_secs() - TIMESTAMP_SKEW_SECONDS - 1;
        let (typed, sig) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, stale, chain_id);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — issued_at outside skew window");
        assert!(format!("{err}").contains("timestamp"));
    }

    #[test]
    fn rejects_timestamp_too_far_future() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        // Pick a timestamp comfortably past the skew window. A tight `+ 1`
        // margin is flaky: the validator reads its own `now`, and if the wall
        // clock advances even one second between here and that read (easy under
        // CI load), `|now - issued_at|` lands exactly on TIMESTAMP_SKEW_SECONDS,
        // which passes the strict `>` check and the timestamp is wrongly
        // accepted. An hour of headroom can't be eroded by test-execution slop.
        // (The mirror test `rejects_timestamp_too_old` is naturally robust —
        // elapsed time only makes its timestamp staler.)
        let future = now_secs() + TIMESTAMP_SKEW_SECONDS + 3600;
        let (typed, sig) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, future, chain_id);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — issued_at too far in future");
        assert!(format!("{err}").contains("timestamp"));
    }

    /// Defense-in-depth: a malicious client can hand-craft `issuedAt` as
    /// `i64::MIN` after the fact (the wallet would refuse, but the JSON
    /// path is unauthenticated). Now rejected one layer earlier at
    /// `extract_issued_at` since `uint256` can't hold negatives, so it
    /// never reaches `validate_timestamp` where the i128 widening lives.
    /// Both layers are defensive; this test pins the earlier rejection.
    #[test]
    fn rejects_i64_min_issued_at() {
        let chain_id = ensure_test_chain_id();
        let typed = serde_json::json!({
            "types": {
                "EIP712Domain": [
                    { "name": "name", "type": "string" },
                    { "name": "version", "type": "string" },
                    { "name": "chainId", "type": "uint256" },
                ],
                "CreateWallet": [
                    { "name": "address", "type": "address" },
                    { "name": "issuedAt", "type": "uint256" },
                ],
            },
            "primaryType": "CreateWallet",
            "domain": {
                "name": "Lit ChainSecured",
                "version": "1",
                "chainId": chain_id.to_string(),
            },
            "message": {
                "address": "0x0000000000000000000000000000000000000000",
                "issuedAt": i64::MIN.to_string(),
            }
        });
        let err = verify_eip712_signature(&typed, "0x00", PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — issued_at is negative, not a valid uint256");
        assert!(format!("{err}").contains("issuedAt"));
    }

    /// `issuedAt` greater than `i64::MAX` would overflow the i64 skew
    /// arithmetic. `extract_issued_at` rejects this before any timestamp
    /// math runs.
    #[test]
    fn rejects_issued_at_overflows_i64() {
        let chain_id = ensure_test_chain_id();
        // i64::MAX is 9223372036854775807; pick something well above it
        // that still fits in uint256.
        let huge = "99999999999999999999"; // > i64::MAX
        let typed = serde_json::json!({
            "types": {
                "EIP712Domain": [
                    { "name": "name", "type": "string" },
                    { "name": "version", "type": "string" },
                    { "name": "chainId", "type": "uint256" },
                ],
                "CreateWallet": [
                    { "name": "address", "type": "address" },
                    { "name": "issuedAt", "type": "uint256" },
                ],
            },
            "primaryType": "CreateWallet",
            "domain": {
                "name": "Lit ChainSecured",
                "version": "1",
                "chainId": chain_id.to_string(),
            },
            "message": {
                "address": "0x0000000000000000000000000000000000000000",
                "issuedAt": huge,
            }
        });
        let err = verify_eip712_signature(&typed, "0x00", PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — issuedAt out of i64 range");
        assert!(format!("{err}").contains("out of range"));
    }

    /// CPL-286 hardening (codex adversarial review): an EIP-712 field
    /// declaration must contain only `name` and `type`. A phishing dApp
    /// could otherwise smuggle a decoy key like `label: "Approve $500"`
    /// that some wallet UIs surface to the user — the EIP-712 type hash
    /// only commits to `(name, type)`, so the digest still recovers and
    /// the canonical schema-equality check would silently pass.
    #[test]
    fn rejects_extra_key_in_field_def() {
        let chain_id = ensure_test_chain_id();
        let typed = serde_json::json!({
            "types": {
                "EIP712Domain": [
                    { "name": "name", "type": "string" },
                    { "name": "version", "type": "string" },
                    { "name": "chainId", "type": "uint256" },
                ],
                "CreateWallet": [
                    { "name": "address", "type": "address", "label": "Approve $500" },
                    { "name": "issuedAt", "type": "uint256" },
                ],
            },
            "primaryType": "CreateWallet",
            "domain": {
                "name": "Lit ChainSecured",
                "version": "1",
                "chainId": chain_id.to_string(),
            },
            "message": {
                "address": "0x0000000000000000000000000000000000000000",
                "issuedAt": now_secs().to_string(),
            }
        });
        let err = verify_eip712_signature(&typed, "0x00", PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — field def has an extra key");
        // serde's deny_unknown_fields error message includes the key name.
        assert!(
            format!("{err}").contains("label") || format!("{err}").contains("unknown field"),
            "unexpected error: {err}",
        );
    }

    /// Same hardening, domain side: anything beyond `(name, version,
    /// chainId, verifyingContract, salt)` is rejected. The
    /// `verifying_contract`/`salt` *presence* still hits the more specific
    /// error message; truly unknown keys (e.g. `displayName`) hit
    /// `deny_unknown_fields` here.
    #[test]
    fn rejects_extra_key_in_domain() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        typed["domain"]["displayName"] = serde_json::json!("Approve transfer");
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — unknown domain key");
        assert!(
            format!("{err}").contains("displayName") || format!("{err}").contains("unknown field"),
            "unexpected error: {err}",
        );
        // Suppress unused-variable warning for chain_id when wallet is used.
        let _ = chain_id;
    }

    /// Accept the `eth_signTypedData_v4` wire shape where `typed_data`
    /// is a JSON string containing the typed-data object (as opposed to
    /// the object inline). Both ethers' and alloy's `TypedData::Deserialize`
    /// accept both shapes; we preserve that here. Otherwise any client
    /// forwarding the metamask response verbatim would break.
    #[test]
    fn accepts_stringified_typed_data() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (json, sig) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        let stringified = serde_json::Value::String(serde_json::to_string(&json).unwrap());
        let recovered = verify_eip712_signature(&stringified, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect("stringified typed_data must verify");
        assert_eq!(recovered, wallet.address());
    }

    /// `issuedAt` declared `uint256` can be a hex string. The verifier
    /// must accept it; alloy's digest computation does, so rejecting it
    /// pre-recovery would silently break otherwise-valid signatures.
    #[test]
    fn accepts_hex_issued_at() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        // Build canonical typed data, then rewrite issuedAt as hex.
        let mut json = build_canonical_typed_data_json(
            PRIMARY_TYPE_CREATE_WALLET,
            wallet.address(),
            now_secs(),
            chain_id,
        );
        let now = now_secs();
        json["message"]["issuedAt"] = serde_json::json!(format!("0x{:x}", now));
        // Sign the digest of the rewritten payload — that's what a wallet
        // signing the hex form would produce.
        let typed_data: TypedData = serde_json::from_value(json.clone()).unwrap();
        let digest = typed_data.eip712_signing_hash().unwrap();
        let sig = wallet.sign_hash_sync(&digest).unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));
        let recovered = verify_eip712_signature(&json, &sig_hex, PRIMARY_TYPE_CREATE_WALLET)
            .expect("hex issuedAt must verify");
        assert_eq!(recovered, wallet.address());
    }

    #[test]
    fn rejects_address_signer_mismatch() {
        let chain_id = ensure_test_chain_id();
        let signer = PrivateKeySigner::random();
        let other = PrivateKeySigner::random();
        // typed data claims `other.address()` but is signed by `signer`.
        let json = build_canonical_typed_data_json(
            PRIMARY_TYPE_CREATE_WALLET,
            other.address(),
            now_secs(),
            chain_id,
        );
        let typed_data: TypedData = serde_json::from_value(json.clone()).unwrap();
        let digest = typed_data.eip712_signing_hash().unwrap();
        let sig = signer.sign_hash_sync(&digest).unwrap();
        let err = verify_eip712_signature(
            &json,
            &format!("0x{}", hex::encode(sig.as_bytes())),
            PRIMARY_TYPE_CREATE_WALLET,
        )
        .expect_err("must reject — recovered signer ≠ claimed address");
        assert!(format!("{err}").contains("Signature does not match"));
    }

    #[test]
    fn rejects_bad_signature_hex() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (typed, _) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        let err = verify_eip712_signature(&typed, "0xnothex", PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — bad signature hex");
        assert!(format!("{err}").contains("signature") || format!("{err}").contains("Signature"));
    }

    #[test]
    fn rejects_malformed_typed_data_json() {
        let _ = ensure_test_chain_id();
        let bogus = serde_json::json!({ "not": "a typed data object" });
        let err = verify_eip712_signature(&bogus, "0x00", PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — JSON does not deserialize as TypedData");
        assert!(format!("{err}").to_lowercase().contains("typed_data"));
    }

    #[test]
    fn rejects_oversized_typed_data() {
        let _ = ensure_test_chain_id();
        // Pad the domain.name with a giant string to blow past the cap. The
        // domain validator runs after the size check, so this hits the size
        // cap first (a strict-message-schema test below covers extras inside
        // `message` separately, which now gets rejected by validate_type_schema
        // before the size cap can be reached on a small payload).
        let mut typed = serde_json::json!({
            "types": {
                "EIP712Domain": [
                    { "name": "name", "type": "string" },
                    { "name": "version", "type": "string" },
                    { "name": "chainId", "type": "uint256" },
                ],
                "CreateWallet": [
                    { "name": "address", "type": "address" },
                    { "name": "issuedAt", "type": "uint256" },
                ],
            },
            "primaryType": "CreateWallet",
            "domain": {
                "name": "Lit ChainSecured",
                "version": "1",
                "chainId": "175188",
            },
            "message": {
                "address": "0x0000000000000000000000000000000000000000",
                "issuedAt": "0",
            }
        });
        typed["domain"]["name"] = serde_json::Value::String("x".repeat(MAX_TYPED_DATA_LEN + 1));
        let err = verify_eip712_signature(&typed, "0x00", PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — typed_data exceeds size cap");
        assert!(format!("{err}").contains("too large"));
    }

    /// CPL-286 hardening (Codex adversarial review): reject typed-data
    /// `message` blobs that contain fields beyond the canonical
    /// `(address, issuedAt)` pair. Extras don't change the EIP-712 digest
    /// (they aren't in the type hash), but a phishing dApp could embed a
    /// decoy field like `intent: "Subscribe to weekly briefing"` that some
    /// wallet UIs render alongside the canonical fields, tricking the user
    /// into signing what looks like a different action.
    #[test]
    fn rejects_extra_message_fields() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        typed["message"]["decoy"] = serde_json::json!("Authorize $10 subscription");
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — message must contain only address + issuedAt");
        assert!(format!("{err}").contains("unexpected fields"));
    }

    /// An oversized signature is rejected before any decode/recovery work and
    /// before reaching the (unauthenticated) EIP-1271 RPC path — bounds the
    /// allocation + calldata an attacker can force on the mint endpoints.
    #[test]
    fn rejects_oversized_signature() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (typed, _) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        let huge_sig = format!("0x{}", "ab".repeat(MAX_SIGNATURE_BYTES + 1));
        let err = verify_eip712_signature(&typed, &huge_sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — signature exceeds cap");
        assert!(
            format!("{err}").contains("too large"),
            "unexpected error: {err}"
        );
    }

    /// The ERC-1271 magic value is `bytes4(keccak256("isValidSignature(bytes32,bytes)"))`.
    /// Pin the literal so a typo can't silently make every contract-wallet
    /// signature "valid" (or "invalid").
    #[test]
    fn erc1271_magic_value_is_correct() {
        use alloy::primitives::keccak256;
        let selector = &keccak256("isValidSignature(bytes32,bytes)")[..4];
        assert_eq!(selector, ERC1271_MAGIC_VALUE);
    }

    /// The contract-wallet-aware entry point must short-circuit on a valid EOA
    /// (ECDSA) signature without ever touching the chain — the read-only client
    /// is never initialised in unit tests, so reaching the EIP-1271 path here
    /// would surface as an error instead of `Ok`.
    #[tokio::test]
    async fn allow_contract_wallet_accepts_eoa_without_rpc() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        let (typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CONVERT_ACCOUNT, now_secs(), chain_id);
        let recovered = verify_eip712_signature_allow_contract_wallet(
            &typed,
            &sig,
            PRIMARY_TYPE_CONVERT_ACCOUNT,
        )
        .await
        .expect("valid EOA signature must verify via the ECDSA short-circuit");
        assert_eq!(recovered, wallet.address());
    }

    /// A non-65-byte (contract-wallet-shaped) signature does not recover via
    /// ECDSA, so the contract-aware entry point falls through to the on-chain
    /// EIP-1271 path. With no read-only client initialised in tests, that
    /// surfaces as an error rather than a panic — and crucially is *not*
    /// silently accepted.
    #[tokio::test]
    async fn allow_contract_wallet_falls_through_to_1271_for_non_ecdsa_sig() {
        let chain_id = ensure_test_chain_id();
        let wallet = PrivateKeySigner::random();
        // Build a valid canonical payload but replace the signature with a
        // contract-wallet-shaped blob (not 65 bytes).
        let (typed, _) =
            sign_canonical(&wallet, PRIMARY_TYPE_CONVERT_ACCOUNT, now_secs(), chain_id);
        let contract_sig = format!("0x{}", "ab".repeat(100)); // 100 bytes, not 65
        let err = verify_eip712_signature_allow_contract_wallet(
            &typed,
            &contract_sig,
            PRIMARY_TYPE_CONVERT_ACCOUNT,
        )
        .await
        .expect_err("must not accept a contract-wallet sig with no chain client to verify against");
        // Reached the 1271 path (client-unavailable), not a parse/validation error.
        assert!(
            format!("{err}").contains("Chain client unavailable"),
            "unexpected error: {err}",
        );
    }
}
