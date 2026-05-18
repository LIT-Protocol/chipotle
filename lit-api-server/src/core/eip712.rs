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

use ethers::core::types::transaction::eip712::{EIP712Domain, Eip712, Eip712DomainType, TypedData};
use ethers::core::types::{Address, RecoveryMessage, Signature as EthSignature, U256};

use crate::config::GLOBAL_NODE_CONFIG;
use crate::core::v1::helpers::api_status::ApiStatus;

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

/// The four flows share an identical message struct: `(address, issuedAt)`.
/// Field declaration order is part of the EIP-712 type hash — clients must
/// declare the fields in this order, and any reordering is rejected by
/// `validate_type_schema`.
fn payload_field_schema() -> [Eip712DomainType; 2] {
    [
        Eip712DomainType {
            name: "address".to_string(),
            r#type: "address".to_string(),
        },
        Eip712DomainType {
            name: "issuedAt".to_string(),
            r#type: "uint256".to_string(),
        },
    ]
}

/// Canonical `EIP712Domain` field declaration. We use the (name, version,
/// chainId) subset — no `verifyingContract` (this server is not an
/// on-chain contract) and no `salt`. Clients must declare these in the
/// same order; any other shape is rejected.
fn domain_field_schema() -> [Eip712DomainType; 3] {
    [
        Eip712DomainType {
            name: "name".to_string(),
            r#type: "string".to_string(),
        },
        Eip712DomainType {
            name: "version".to_string(),
            r#type: "string".to_string(),
        },
        Eip712DomainType {
            name: "chainId".to_string(),
            r#type: "uint256".to_string(),
        },
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

/// Build the canonical typed data the server expects for a given flow.
/// Used by tests to round-trip a wallet signature; not part of the request
/// path (the request path validates the client-supplied typed data instead
/// of rebuilding it, so the wallet's view and the server's view are bit-
/// identical when verification succeeds).
#[cfg(test)]
pub(crate) fn build_canonical_typed_data(
    primary_type: &str,
    address: Address,
    issued_at: i64,
    chain_id: u64,
) -> TypedData {
    use std::collections::BTreeMap;

    let mut types: BTreeMap<String, Vec<Eip712DomainType>> = BTreeMap::new();
    types.insert("EIP712Domain".to_string(), domain_field_schema().to_vec());
    types.insert(primary_type.to_string(), payload_field_schema().to_vec());

    let mut message: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    message.insert(
        "address".to_string(),
        serde_json::Value::String(format!("0x{:x}", address)),
    );
    message.insert(
        "issuedAt".to_string(),
        serde_json::Value::String(issued_at.to_string()),
    );

    TypedData {
        domain: EIP712Domain {
            name: Some(EIP712_DOMAIN_NAME.to_string()),
            version: Some(EIP712_DOMAIN_VERSION.to_string()),
            chain_id: Some(U256::from(chain_id)),
            verifying_contract: None,
            salt: None,
        },
        types,
        primary_type: primary_type.to_string(),
        message,
    }
}

/// Verify an EIP-712 typed-data + signature pair for a specific ChainSecured
/// flow. Returns the recovered wallet address on success.
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

    let typed_data: TypedData = serde_json::from_value(typed_data_json.clone()).map_err(|e| {
        ApiStatus::bad_request(
            anyhow::anyhow!(e),
            "typed_data is not a valid EIP-712 typed-data object",
        )
    })?;

    if typed_data.primary_type != expected_primary_type {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "primaryType mismatch: typed_data says {:?}, this endpoint expects {:?}",
                typed_data.primary_type,
                expected_primary_type
            ),
            "primaryType mismatch — signature was minted for a different flow",
        ));
    }
    if !is_known_primary_type(&typed_data.primary_type) {
        // Defence in depth — `expected_primary_type` is always one of our
        // constants, but pin the wider invariant explicitly so adding a new
        // primary type forces us to update `is_known_primary_type` too.
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Unknown primaryType {:?}", typed_data.primary_type),
            "Unknown primaryType",
        ));
    }

    validate_domain(&typed_data.domain)?;
    validate_type_schema(&typed_data)?;
    let address = extract_address(&typed_data)?;
    let issued_at = extract_issued_at(&typed_data)?;
    validate_timestamp(issued_at)?;

    let digest = typed_data.encode_eip712().map_err(|e| {
        ApiStatus::internal_server_error(
            anyhow::anyhow!("encode_eip712 failed: {}", e),
            "encode_eip712 failed",
        )
    })?;
    let sig: EthSignature =
        signature_hex
            .trim()
            .parse()
            .map_err(|e: ethers::core::types::SignatureError| {
                ApiStatus::bad_request(anyhow::anyhow!(e), "Invalid signature hex")
            })?;
    let recovered = sig
        .recover(RecoveryMessage::Hash(digest.into()))
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "Signature recovery failed"))?;

    if recovered != address {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Signature does not match claimed address"),
            "Signature does not match claimed address",
        ));
    }
    Ok(address)
}

fn validate_domain(domain: &EIP712Domain) -> Result<(), ApiStatus> {
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
    if domain.chain_id != Some(expected_chain_id) {
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

fn validate_type_schema(typed_data: &TypedData) -> Result<(), ApiStatus> {
    let expected_payload = payload_field_schema();
    let primary_decl = typed_data
        .types
        .get(&typed_data.primary_type)
        .ok_or_else(|| {
            ApiStatus::bad_request(
                anyhow::anyhow!("Missing types[{}]", typed_data.primary_type),
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
    let domain_decl = typed_data.types.get("EIP712Domain").ok_or_else(|| {
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
    if typed_data.types.len() != 2 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Unexpected type definitions — expected only EIP712Domain and {}",
                typed_data.primary_type
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
    if typed_data.message.len() != 2
        || !typed_data.message.contains_key("address")
        || !typed_data.message.contains_key("issuedAt")
    {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "message must contain exactly `address` and `issuedAt`, got {:?}",
                typed_data.message.keys().collect::<Vec<_>>()
            ),
            "message contains unexpected fields",
        ));
    }
    Ok(())
}

fn extract_address(typed_data: &TypedData) -> Result<Address, ApiStatus> {
    let address_val = typed_data.message.get("address").ok_or_else(|| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Missing message.address"),
            "Missing message.address",
        )
    })?;
    let address: Address = serde_json::from_value(address_val.clone())
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "Invalid message.address"))?;
    Ok(address)
}

fn extract_issued_at(typed_data: &TypedData) -> Result<i64, ApiStatus> {
    let issued_val = typed_data.message.get("issuedAt").ok_or_else(|| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Missing message.issuedAt"),
            "Missing message.issuedAt",
        )
    })?;
    // ethers JS serializes uint256 as a string for typed data, but a small
    // integer fits in JSON numbers — accept both shapes so the client can
    // pick whichever is most natural for its stack.
    match issued_val {
        serde_json::Value::String(s) => s.parse::<i64>().map_err(|e| {
            ApiStatus::bad_request(anyhow::anyhow!(e), "issuedAt not a unix timestamp")
        }),
        serde_json::Value::Number(n) => n.as_i64().ok_or_else(|| {
            ApiStatus::bad_request(
                anyhow::anyhow!("issuedAt not a unix timestamp"),
                "issuedAt not a unix timestamp",
            )
        }),
        _ => Err(ApiStatus::bad_request(
            anyhow::anyhow!("issuedAt must be a number or numeric string"),
            "issuedAt must be a number or numeric string",
        )),
    }
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
    use ethers::core::types::H256;
    use ethers::signers::{LocalWallet, Signer};

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
        wallet: &LocalWallet,
        primary_type: &str,
        issued_at: i64,
        chain_id: u64,
    ) -> (serde_json::Value, String) {
        let typed_data =
            build_canonical_typed_data(primary_type, wallet.address(), issued_at, chain_id);
        let digest = typed_data.encode_eip712().unwrap();
        let sig = wallet.sign_hash(H256::from(digest)).unwrap();
        let json = serde_json::to_value(&typed_data).unwrap();
        (json, format!("0x{}", sig))
    }

    #[test]
    fn happy_path_create_wallet() {
        let chain_id = ensure_test_chain_id();
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
            let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let stale = now_secs() - TIMESTAMP_SKEW_SECONDS - 1;
        let (typed, sig) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, stale, chain_id);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — issued_at outside skew window");
        assert!(format!("{err}").contains("timestamp"));
    }

    #[test]
    fn rejects_timestamp_too_far_future() {
        let chain_id = ensure_test_chain_id();
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let future = now_secs() + TIMESTAMP_SKEW_SECONDS + 1;
        let (typed, sig) = sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, future, chain_id);
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — issued_at too far in future");
        assert!(format!("{err}").contains("timestamp"));
    }

    /// `i64::MIN` would wrap on naive `(now - issued_at).abs()` in release
    /// builds, which would let an attacker bypass the skew check. We can't
    /// actually sign such typed data (uint256 can't hold negatives — the
    /// wallet would refuse), but a malicious client can hand-craft the JSON
    /// after the fact. This pins the i128 widening fix in `validate_timestamp`.
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
            .expect_err("must reject — issued_at saturates to i64::MIN");
        assert!(format!("{err}").contains("timestamp"));
    }

    #[test]
    fn rejects_address_signer_mismatch() {
        let chain_id = ensure_test_chain_id();
        let signer = LocalWallet::new(&mut rand::thread_rng());
        let other = LocalWallet::new(&mut rand::thread_rng());
        // typed data claims `other.address()` but is signed by `signer`.
        let typed_data = build_canonical_typed_data(
            PRIMARY_TYPE_CREATE_WALLET,
            other.address(),
            now_secs(),
            chain_id,
        );
        let digest = typed_data.encode_eip712().unwrap();
        let sig = signer.sign_hash(H256::from(digest)).unwrap();
        let json = serde_json::to_value(&typed_data).unwrap();
        let err = verify_eip712_signature(&json, &format!("0x{}", sig), PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — recovered signer ≠ claimed address");
        assert!(format!("{err}").contains("Signature does not match"));
    }

    #[test]
    fn rejects_bad_signature_hex() {
        let chain_id = ensure_test_chain_id();
        let wallet = LocalWallet::new(&mut rand::thread_rng());
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
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let (mut typed, sig) =
            sign_canonical(&wallet, PRIMARY_TYPE_CREATE_WALLET, now_secs(), chain_id);
        typed["message"]["decoy"] = serde_json::json!("Authorize $10 subscription");
        let err = verify_eip712_signature(&typed, &sig, PRIMARY_TYPE_CREATE_WALLET)
            .expect_err("must reject — message must contain only address + issuedAt");
        assert!(format!("{err}").contains("unexpected fields"));
    }
}
