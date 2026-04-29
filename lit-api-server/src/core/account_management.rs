use std::sync::Arc;

use crate::accounts::chain_config::config_key_names;
use crate::accounts::signer_pool::SignerPool;
use crate::config::GLOBAL_NODE_CONFIG;
use crate::core::v1::helpers::api_status::ApiStatus;
use crate::core::v1::models::request::{
    AddActionRequest, AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest,
    AddUsageApiKeyRequest, AddUsageApiKeyWithSignatureRequest, ConvertToChainSecuredAccountRequest,
    CreateWalletWithSignatureRequest, DeleteActionRequest, NewAccountRequest,
    RemoveActionFromGroupRequest, RemoveGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest, UpdateUsageApiKeyRequest,
};
use crate::core::v1::models::response::{
    AccountOpResponse, AddGroupResponse, AddUsageApiKeyResponse,
    AddUsageApiKeyWithSignatureResponse, ApiKeyItem, ChainConfigKeysResponse, CreateWalletResponse,
    CreateWalletWithSignatureResponse, ListMetadataItem, NewAccountResponse,
    NodeChainConfigResponse, WalletItem,
};
use crate::dstack::v1::get_client_key;
use crate::stripe::StripeState;
use crate::utils::generate_unique_derivation_path;
use crate::utils::parse_with_hash::{
    hashed_cid_to_u256, hex_array_to_h160_array, hex_array_to_u256_array, ipfs_cid_to_u256,
    string_group_id_to_u256,
};
use crate::{accounts, dstack};
use elliptic_curve::group::GroupEncoding;
use ethers::core::types::Signature as EthSignature;
use ethers::signers::{LocalWallet, Signer};
use ethers::types::{H160, U256};
use ipfs_hasher::IpfsHasher;
use lit_core::utils::binary::{bytes_to_0x_hex, hex_to_bytes};
use rocket::serde::json::Json;

/// Map a contract error to the appropriate HTTP status.
/// Permission-related contract reverts become 403 Forbidden; everything else stays 500.
///
/// NOTE: This matches on the stringified error from `decode_contract_revert` output.
/// A more robust approach would decode the ABI revert selector bytes directly, but
/// that requires regenerating Rust bindings (AccountConfig.json → account_config_contract.rs)
/// to include the NotAllowedTo* error types. Track as follow-up.
const PERMISSION_ERROR_PATTERNS: &[&str] = &["NotAllowedTo", "NotMasterAccount", "NoAccountAccess"];

fn map_contract_error(e: anyhow::Error, context: &str) -> ApiStatus {
    let msg = format!("{}", e);
    if PERMISSION_ERROR_PATTERNS
        .iter()
        .any(|pat| msg.contains(pat))
    {
        tracing::warn!("Permission denied for {context}: {msg}");
        ApiStatus::forbidden("Permission denied".to_string())
    } else {
        ApiStatus::internal_server_error(e, context)
    }
}

// Create a new wallet and return the public key, wallet address, and secret.
async fn create_new_wallet() -> Result<(String, H160, [u8; 32], U256), ApiStatus> {
    let (derivation_u256, derivation_path) = generate_unique_derivation_path();
    tracing::info!(
        "Creating new wallet with derivation path: {}",
        derivation_path
    );
    let secret: [u8; 32] = get_client_key(&derivation_path).await.map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "get_client_key failed")
    })?;

    let local_wallet = LocalWallet::from_bytes(&secret).map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "LocalWallet::from_bytes failed")
    })?;
    let wallet_address = local_wallet.address();
    let public_key_bytes = local_wallet.signer().verifying_key().as_affine().to_bytes();
    let public_key_string = bytes_to_0x_hex(public_key_bytes);

    Ok((public_key_string, wallet_address, secret, derivation_u256))
}

pub async fn new_account(
    signer_pool: Arc<SignerPool>,
    stripe_state: Option<Arc<StripeState>>,
    new_account_request: Json<NewAccountRequest>,
) -> Result<NewAccountResponse, ApiStatus> {
    let account_name = new_account_request.account_name.clone();
    let account_description = new_account_request.account_description.clone();
    let email = new_account_request.email.clone().unwrap_or_default();

    let (_public_key, wallet_address, secret, derivation_path) = create_new_wallet().await?;
    let api_key = base64_light::base64_encode_bytes(&secret);

    if let Err(e) = accounts::new_account(
        signer_pool.clone(),
        &api_key,
        &account_name,
        &account_description,
        wallet_address,
    )
    .await
    {
        return Err(e.into());
    }

    // technically this is NOT a derivaton path at all, but it's a stand-in for now
    accounts::register_wallet_derivation(
        signer_pool,
        &api_key,
        wallet_address,
        derivation_path,
        "AMW",
        "Account Master Wallet",
    )
    .await?;

    // Best-effort: eagerly create the Stripe customer (with $0 balance) and set the email
    // if provided.  Neither failure should prevent account creation.
    if let Some(stripe) = stripe_state {
        let wallet_hex = bytes_to_0x_hex(wallet_address.as_bytes());
        match crate::stripe::get_customer_by_wallet(&wallet_hex, &stripe).await {
            Ok(customer_id) => {
                if !email.trim().is_empty() {
                    let _ = crate::stripe::set_customer_email(&customer_id, email.trim(), &stripe)
                        .await;
                }
            }
            Err(e) => {
                tracing::warn!("stripe: failed to create customer for new account: {e}");
            }
        }
    }

    Ok(NewAccountResponse {
        api_key: api_key.to_string(),
        wallet_address: bytes_to_0x_hex(wallet_address.as_bytes()),
    })
}

pub async fn account_exists(api_key: &str) -> Result<bool, ApiStatus> {
    let exists = accounts::account_exists(api_key)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "account_exists failed"))?;
    Ok(exists)
}

pub async fn create_wallet(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
) -> Result<CreateWalletResponse, ApiStatus> {
    let (_public_key, wallet_address, _secret, derivation_u256) = create_new_wallet().await?;

    tracing::info!("Creating wallet with address: {:?}", wallet_address);
    // technically this is NOT a derivaton path at all, but it's a stand-in for now
    accounts::register_wallet_derivation(
        signer_pool,
        api_key,
        wallet_address,
        derivation_u256,
        "Wallet",
        "Wallet",
    )
    .await
    .map_err(|e| map_contract_error(e, "create_wallet failed"))?;

    Ok(CreateWalletResponse {
        wallet_address: bytes_to_0x_hex(wallet_address.as_bytes()),
    })
}

/// V1 SIWE-lite. Shared by `create_wallet_with_signature` and
/// `add_usage_api_key_with_signature` — both authenticate PKP minting
/// with a wallet signature because ChainSecured users have no api key.
/// The server only mints — the client follows up on-chain with
/// `registerWalletDerivation` (and, for usage keys, `setUsageApiKey`)
/// signed by the admin wallet (which may differ from the SIWE signer).
///
/// Replay: ±5-minute timestamp window, no nonce store. Worst case on
/// either endpoint is an extra unregistered PKP — the bytes the server
/// returns are unattached on-chain until the admin wallet calls the
/// follow-up txns, so they are equivalent to a freshly generated keypair
/// until then. Compute cost only.
const SIWE_TIMESTAMP_SKEW_SECONDS: i64 = 300;

/// Hard cap on the SIWE-lite message size to prevent unauthenticated callers
/// from forcing the server to allocate / hash large strings before any cheap
/// reject. 4 KiB is far above any legitimate three-line message.
const MAX_SIWE_MESSAGE_LEN: usize = 4096;

/// Parsed shape of a ChainSecured SIWE-lite message.
struct ParsedChainsecuredSiwe {
    address: H160,
    chain_id: u64,
    issued_at: i64,
}

/// Parses a ChainSecured SIWE-lite message. Required lines (case-sensitive),
/// each must appear exactly once:
///   `Address: 0x…`
///   `Chain ID: <u64>`
///   `Issued At: <unix-seconds>`
/// Other lines are ignored so callers can include domain/statement framing.
/// Duplicate required lines are rejected (last-wins parsing would let a
/// crafted message show one value to the wallet UI and another to the server).
fn parse_chainsecured_siwe(message: &str) -> Result<ParsedChainsecuredSiwe, ApiStatus> {
    let mut address: Option<H160> = None;
    let mut chain_id: Option<u64> = None;
    let mut issued_at: Option<i64> = None;
    for line in message.lines() {
        if let Some(v) = line.strip_prefix("Address:") {
            if address.is_some() {
                return Err(ApiStatus::bad_request(
                    anyhow::anyhow!("Duplicate Address line"),
                    "Duplicate Address line",
                ));
            }
            let trimmed = v.trim();
            let bytes = hex_to_bytes(trimmed.trim_start_matches("0x")).map_err(|_| {
                ApiStatus::bad_request(
                    anyhow::anyhow!("Address line not valid hex"),
                    "Address line not valid hex",
                )
            })?;
            if bytes.len() != 20 {
                return Err(ApiStatus::bad_request(
                    anyhow::anyhow!("Address must be 20 bytes"),
                    "Address must be 20 bytes",
                ));
            }
            address = Some(H160::from_slice(&bytes));
        } else if let Some(v) = line.strip_prefix("Chain ID:") {
            if chain_id.is_some() {
                return Err(ApiStatus::bad_request(
                    anyhow::anyhow!("Duplicate Chain ID line"),
                    "Duplicate Chain ID line",
                ));
            }
            chain_id = Some(v.trim().parse::<u64>().map_err(|_| {
                ApiStatus::bad_request(
                    anyhow::anyhow!("Chain ID line not a u64"),
                    "Chain ID line not a u64",
                )
            })?);
        } else if let Some(v) = line.strip_prefix("Issued At:") {
            if issued_at.is_some() {
                return Err(ApiStatus::bad_request(
                    anyhow::anyhow!("Duplicate Issued At line"),
                    "Duplicate Issued At line",
                ));
            }
            issued_at = Some(v.trim().parse::<i64>().map_err(|_| {
                ApiStatus::bad_request(
                    anyhow::anyhow!("Issued At line not a unix timestamp"),
                    "Issued At line not a unix timestamp",
                )
            })?);
        }
    }
    Ok(ParsedChainsecuredSiwe {
        address: address.ok_or_else(|| {
            ApiStatus::bad_request(
                anyhow::anyhow!("Missing Address line"),
                "Missing Address line",
            )
        })?,
        chain_id: chain_id.ok_or_else(|| {
            ApiStatus::bad_request(
                anyhow::anyhow!("Missing Chain ID line"),
                "Missing Chain ID line",
            )
        })?,
        issued_at: issued_at.ok_or_else(|| {
            ApiStatus::bad_request(
                anyhow::anyhow!("Missing Issued At line"),
                "Missing Issued At line",
            )
        })?,
    })
}

/// Recovers the EIP-191 personal-sign signer for `message`.
fn recover_eip191_signer(message: &str, signature_hex: &str) -> Result<H160, ApiStatus> {
    let sig: EthSignature =
        signature_hex
            .trim()
            .parse()
            .map_err(|e: ethers::core::types::SignatureError| {
                ApiStatus::bad_request(anyhow::anyhow!(e), "Invalid signature hex")
            })?;
    sig.recover(message.as_bytes().to_vec())
        .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "Signature recovery failed"))
}

/// Parses + verifies a ChainSecured SIWE-lite message and returns the recovered
/// signer. Cheap rejects (length cap, parse, chain ID, timestamp window) run
/// before the expensive ECDSA recovery so attacker traffic with stale or
/// wrong-chain messages is dropped without doing crypto.
fn verify_chainsecured_siwe(message: &str, signature: &str) -> Result<H160, ApiStatus> {
    if message.len() > MAX_SIWE_MESSAGE_LEN {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Message exceeds {}-byte cap", MAX_SIWE_MESSAGE_LEN),
            "Message too large",
        ));
    }

    let parsed = parse_chainsecured_siwe(message)?;

    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))
        .map_err(|e| ApiStatus::internal_server_error(e, "GLOBAL_NODE_CONFIG missing"))?;
    let expected_chain_id = node_config.chain.info().chain_id;
    if parsed.chain_id != expected_chain_id {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Chain ID mismatch: message says {}, server is on {}",
                parsed.chain_id,
                expected_chain_id
            ),
            "Chain ID mismatch",
        ));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!(e),
                "System clock is before the Unix epoch",
            )
        })?
        .as_secs() as i64;
    if (now - parsed.issued_at).abs() > SIWE_TIMESTAMP_SKEW_SECONDS {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Issued At {} is outside the ±{}s window from now ({})",
                parsed.issued_at,
                SIWE_TIMESTAMP_SKEW_SECONDS,
                now
            ),
            "Signed message timestamp is too old or too far in the future",
        ));
    }

    let signer = recover_eip191_signer(message, signature)?;
    if signer != parsed.address {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Signature does not match claimed address"),
            "Signature does not match claimed address",
        ));
    }

    Ok(signer)
}

pub async fn create_wallet_with_signature(
    req: Json<CreateWalletWithSignatureRequest>,
) -> Result<CreateWalletWithSignatureResponse, ApiStatus> {
    let signer = verify_chainsecured_siwe(&req.message, &req.signature)?;
    tracing::info!(
        "create_wallet_with_signature: minting PKP for ChainSecured signer {:?}",
        signer
    );
    let (_public_key, wallet_address, _secret, derivation_u256) = create_new_wallet().await?;
    Ok(CreateWalletWithSignatureResponse {
        wallet_address: bytes_to_0x_hex(wallet_address.as_bytes()),
        derivation_path: format!("0x{:x}", derivation_u256),
    })
}

/// ChainSecured usage API key: server only mints the wallet (PKP) gated by an
/// EIP-191 wallet signature; the client follows up with on-chain
/// `registerWalletDerivation` and `setUsageApiKey` signed by their admin
/// wallet (api_payer cannot call setUsageApiKey for ChainSecured accounts —
/// see AppStorage.accountExistsAndIsMutable).
pub async fn add_usage_api_key_with_signature(
    req: Json<AddUsageApiKeyWithSignatureRequest>,
) -> Result<AddUsageApiKeyWithSignatureResponse, ApiStatus> {
    let signer = verify_chainsecured_siwe(&req.message, &req.signature)?;
    tracing::info!(
        "add_usage_api_key_with_signature: minting usage-key PKP for ChainSecured signer {:?}",
        signer
    );
    let (_public_key, wallet_address, secret, derivation_u256) = create_new_wallet().await?;
    let usage_api_key = base64_light::base64_encode_bytes(&secret);
    Ok(AddUsageApiKeyWithSignatureResponse {
        usage_api_key,
        wallet_address: bytes_to_0x_hex(wallet_address.as_bytes()),
        derivation_path: format!("0x{:x}", derivation_u256),
    })
}

/// Convert a managed (API-mode) account into a ChainSecured (sovereign) account by
/// reassigning its admin wallet to a user-controlled address. The user proves
/// ownership of `new_admin_wallet_address` with a SIWE-style EIP-191 signature
/// (same format as `create_wallet_with_signature`). The api_payer signs the
/// on-chain `convertToChainSecuredAccount` call.
///
/// Irreversible: the contract reverts if the account is already ChainSecured.
pub async fn convert_to_chain_secured_account(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<ConvertToChainSecuredAccountRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let claimed_address_bytes = hex_to_bytes(req.new_admin_wallet_address.trim_start_matches("0x"))
        .map_err(|_| {
            ApiStatus::bad_request(
                anyhow::anyhow!("new_admin_wallet_address is not valid hex"),
                "new_admin_wallet_address is not valid hex",
            )
        })?;
    if claimed_address_bytes.len() != 20 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("new_admin_wallet_address must be 20 bytes"),
            "new_admin_wallet_address must be 20 bytes",
        ));
    }
    let claimed_address = H160::from_slice(&claimed_address_bytes);
    if claimed_address == H160::zero() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("new_admin_wallet_address must be non-zero"),
            "new_admin_wallet_address must be non-zero",
        ));
    }

    let parsed = parse_chainsecured_siwe(&req.message)?;
    if parsed.address != claimed_address {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Signed Address does not match new_admin_wallet_address"),
            "Signed Address does not match new_admin_wallet_address",
        ));
    }
    let signer = recover_eip191_signer(&req.message, &req.signature)?;
    if signer != claimed_address {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Signature does not match new_admin_wallet_address"),
            "Signature does not match new_admin_wallet_address",
        ));
    }

    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))
        .map_err(|e| ApiStatus::internal_server_error(e, "GLOBAL_NODE_CONFIG missing"))?;
    let expected_chain_id = node_config.chain.info().chain_id;
    if parsed.chain_id != expected_chain_id {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Chain ID mismatch: message says {}, server is on {}",
                parsed.chain_id,
                expected_chain_id
            ),
            "Chain ID mismatch",
        ));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!(e),
                "System clock is before the Unix epoch",
            )
        })?
        .as_secs() as i64;
    if (now - parsed.issued_at).abs() > SIWE_TIMESTAMP_SKEW_SECONDS {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!(
                "Issued At {} is outside the ±{}s window from now ({})",
                parsed.issued_at,
                SIWE_TIMESTAMP_SKEW_SECONDS,
                now
            ),
            "Signed message timestamp is too old or too far in the future",
        ));
    }

    accounts::convert_to_chain_secured_account(signer_pool, api_key, claimed_address)
        .await
        .map_err(|e| map_contract_error(e, "convert_to_chain_secured_account failed"))?;

    Ok(AccountOpResponse { success: true })
}

pub async fn get_lit_action_ipfs_id(code: String) -> Result<String, ApiStatus> {
    let ipfs_hasher = IpfsHasher::default();
    let derived_ipfs_id = ipfs_hasher.compute(code.as_bytes());
    Ok(derived_ipfs_id)
}

#[tracing::instrument(name = "account_management::add_group", skip(signer_pool, api_key))]
pub async fn add_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddGroupRequest>,
) -> Result<AddGroupResponse, ApiStatus> {
    let cid_hashes = hex_array_to_u256_array(&req.cid_hashes_permitted)?;
    let pkp_ids = hex_array_to_h160_array(&req.pkp_ids_permitted)?;

    let group_id = accounts::add_group(
        signer_pool,
        api_key,
        &req.group_name,
        &req.group_description,
        cid_hashes,
        pkp_ids,
    )
    .await
    .map_err(|e| map_contract_error(e, "add_group failed"))?;
    Ok(AddGroupResponse {
        success: true,
        group_id: group_id.to_string(),
    })
}

pub async fn add_action(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddActionRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let action_hash = ipfs_cid_to_u256(&req.action_ipfs_cid)?;
    accounts::add_action(signer_pool, api_key, action_hash, req.into_inner())
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "add_action failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn delete_action(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<DeleteActionRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let action_hash = hashed_cid_to_u256(&req.hashed_cid)?;
    if action_hash == U256::zero() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Cannot remove action with hash 0x0"),
            "Cannot remove action with hash 0x0",
        ));
    }
    accounts::remove_action(signer_pool, api_key, action_hash)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "delete_action failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_action_to_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddActionToGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = U256::from(req.group_id);
    accounts::add_action_to_group(signer_pool, api_key, group_id, &req.action_ipfs_cid)
        .await
        .map_err(|e| map_contract_error(e, "add_action_to_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_pkp_to_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddPkpToGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = U256::from(req.group_id);
    let wallet_address_bytes = hex_to_bytes(&req.pkp_id)?;
    if wallet_address_bytes.len() != 20 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Invalid PKP ID"),
            "Invalid PKP ID",
        ));
    }
    let wallet_address = H160::from_slice(&wallet_address_bytes);
    accounts::add_pkp_to_group(signer_pool, api_key, group_id, wallet_address)
        .await
        .map_err(|e| map_contract_error(e, "add_pkp_to_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn remove_pkp_from_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<RemovePkpFromGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = U256::from(req.group_id);
    let src = hex_to_bytes(&req.pkp_id)?;
    if src.len() != 20 {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Invalid PKP ID"),
            "Invalid PKP ID",
        ));
    }
    let wallet_address = H160::from_slice(&src);
    accounts::remove_pkp_from_group(signer_pool, api_key, group_id, wallet_address)
        .await
        .map_err(|e| map_contract_error(e, "remove_pkp_from_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

#[tracing::instrument(
    name = "account_management::add_usage_api_key",
    skip(signer_pool, api_key)
)]
pub async fn add_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddUsageApiKeyRequest>,
) -> Result<AddUsageApiKeyResponse, ApiStatus> {
    let ten_years_from_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!(e),
                "System clock is before the Unix epoch",
            )
        })?
        .as_secs()
        + 3600 * 24 * 365 * 10;
    let expiration = U256::from(ten_years_from_now);
    let balance = U256::from(10000000);

    let (_public_key, wallet_address, secret, derivation_u256) = create_new_wallet().await?;

    // technically this is NOT a derivaton path at all, but it's a stand-in for now

    accounts::register_wallet_derivation(
        signer_pool.clone(),
        api_key,
        wallet_address,
        derivation_u256,
        "API Key Wallet",
        "Usage API Key Wallet",
    )
    .await?;

    let usage_api_key = base64_light::base64_encode_bytes(&secret);

    accounts::add_usage_api_key(
        signer_pool,
        api_key,
        &usage_api_key,
        expiration,
        balance,
        req.into_inner(),
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "add_usage_api_key failed"))?;
    Ok(AddUsageApiKeyResponse {
        success: true,
        usage_api_key,
    })
}

pub async fn remove_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<RemoveGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = string_group_id_to_u256(&req.group_id)?;
    if group_id == U256::zero() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Cannot remove group with ID 0"),
            "Cannot remove group with ID 0",
        ));
    }
    accounts::remove_group(signer_pool, api_key, group_id)
        .await
        .map_err(|e| map_contract_error(e, "remove_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn remove_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<RemoveUsageApiKeyRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    accounts::remove_usage_api_key(signer_pool, api_key, &req.usage_api_key)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "remove_usage_api_key failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn update_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<UpdateUsageApiKeyRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let ten_years_from_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!(e),
                "System clock is before the Unix epoch",
            )
        })?
        .as_secs()
        + 3600 * 24 * 365 * 10;
    let expiration = U256::from(ten_years_from_now);
    let balance = U256::from(10000000u64);
    let usage_api_key = req.usage_api_key.clone();
    accounts::update_usage_api_key(
        signer_pool,
        api_key,
        &usage_api_key,
        expiration,
        balance,
        req.into_inner(),
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "update_usage_api_key failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn update_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<UpdateGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = U256::from(req.group_id);
    if group_id == U256::zero() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Cannot update group with ID 0"),
            "Cannot update group with ID 0",
        ));
    }
    let cid_hashes = hex_array_to_u256_array(&req.cid_hashes_permitted)?;
    let pkp_ids = hex_array_to_h160_array(&req.pkp_ids_permitted)?;
    accounts::update_group(
        signer_pool,
        api_key,
        group_id,
        &req.name,
        &req.description,
        cid_hashes,
        pkp_ids,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "update_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn remove_action_from_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<RemoveActionFromGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = U256::from(req.group_id);
    if group_id == U256::zero() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Cannot remove action from group with ID 0"),
            "Cannot remove action from group with ID 0",
        ));
    }
    let action_hash = hashed_cid_to_u256(&req.hashed_cid)?;
    accounts::remove_action_from_group(signer_pool, api_key, group_id, action_hash)
        .await
        .map_err(|e| map_contract_error(e, "remove_action_from_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn update_action_metadata(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<UpdateActionMetadataRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let action_hash = hashed_cid_to_u256(&req.hashed_cid)?;
    if action_hash == U256::zero() {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Cannot update action with hash 0x0"),
            "Cannot update action with hash 0x0",
        ));
    }
    accounts::update_action_metadata(
        signer_pool,
        api_key,
        action_hash,
        U256::zero(),
        &req.name,
        &req.description,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "update_action_metadata failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn update_usage_api_key_metadata(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<UpdateUsageApiKeyMetadataRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    accounts::update_usage_api_key_metadata(
        signer_pool,
        api_key,
        &req.usage_api_key,
        &req.name,
        &req.description,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "update_usage_api_key_metadata failed"))?;
    Ok(AccountOpResponse { success: true })
}

#[allow(dead_code)]
fn wallet_metadata_to_item(m: &accounts::Metadata) -> ListMetadataItem {
    metadata_to_item(m, "n/a", "Any", "Any wallet in this account.")
}

fn action_metadata_to_item(m: &accounts::Metadata) -> ListMetadataItem {
    metadata_to_item(m, "n/a", "Any", "Any action received.")
}

fn group_metadata_to_item(m: &accounts::Metadata) -> ListMetadataItem {
    metadata_to_item(m, "n/a", "Any", "Any group in this account.")
}

fn metadata_to_item(
    m: &accounts::Metadata,
    wildcard_id: &str,
    wildcard_name: &str,
    wildcard_description: &str,
) -> ListMetadataItem {
    if m.id == U256::zero() {
        return ListMetadataItem {
            id: wildcard_id.to_string(),
            name: wildcard_name.to_string(),
            description: wildcard_description.to_string(),
        };
    }

    let mut bytes = [0; 32];
    m.id.to_big_endian(&mut bytes);

    ListMetadataItem {
        id: bytes_to_0x_hex(bytes),
        name: m.name.clone(),
        description: m.description.clone(),
    }
}

#[allow(dead_code)]
fn usage_api_key_to_api_key_item(
    m: &accounts::contracts::account_config_contract::UsageApiKeyReturn,
) -> ApiKeyItem {
    let mut bytes = [0; 32];
    m.metadata.id.to_big_endian(&mut bytes);
    let id = bytes_to_0x_hex(bytes);

    let mut hash_bytes = [0; 32];
    m.api_key_hash.to_big_endian(&mut hash_bytes);
    let api_key_hash = bytes_to_0x_hex(hash_bytes);

    ApiKeyItem {
        id,
        api_key_hash,
        name: m.metadata.name.clone(),
        description: m.metadata.description.clone(),
        expiration: m.expiration.to_string(),
        balance: m.balance.as_u64(),
        can_create_groups: m.create_groups,
        can_delete_groups: m.delete_groups,
        can_create_pkps: m.create_pk_ps,
        can_manage_ipfs_ids_in_groups: m
            .manage_ipfs_ids_in_groups
            .iter()
            .map(|id| id.as_u64())
            .collect(),
        can_add_pkp_to_groups: m.add_pkp_to_groups.iter().map(|id| id.as_u64()).collect(),
        can_remove_pkp_from_groups: m
            .remove_pkp_from_groups
            .iter()
            .map(|id| id.as_u64())
            .collect(),
        can_execute_in_groups: m.execute_in_groups.iter().map(|id| id.as_u64()).collect(),
    }
}

pub async fn list_api_keys(
    api_key: &str,
    page_number: u64,
    page_size: u64,
) -> Result<Vec<ApiKeyItem>, ApiStatus> {
    let pn = U256::from(page_number);
    let ps = U256::from(page_size);
    let list = accounts::list_api_keys(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_api_keys failed"))?;

    let api_key_items = list.iter().map(usage_api_key_to_api_key_item).collect();
    Ok(api_key_items)
}

pub async fn list_groups(
    api_key: &str,
    page_number: u64,
    page_size: u64,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let pn = U256::from(page_number);
    let ps = U256::from(page_size);
    let list = accounts::list_groups(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_groups failed"))?;
    Ok(list.iter().map(group_metadata_to_item).collect())
}

pub async fn list_wallets(
    api_key: &str,
    page_number: u64,
    page_size: u64,
) -> Result<Vec<WalletItem>, ApiStatus> {
    let pn = U256::from(page_number);
    let ps = U256::from(page_size);
    let list = accounts::list_wallets(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_wallets failed"))?;

    let wallet_items = list
        .iter()
        .map(|m| WalletItem {
            id: m.id.to_string(),
            name: m.name.clone(),
            description: m.description.clone(),
            wallet_address: bytes_to_0x_hex(m.pkp_id.as_bytes()),
        })
        .collect();
    Ok(wallet_items)
}

pub async fn list_wallets_in_group(
    api_key: &str,
    group_id: u64,
    page_number: u64,
    page_size: u64,
) -> Result<Vec<WalletItem>, ApiStatus> {
    let gid = U256::from(group_id);
    let pn = U256::from(page_number);
    let ps = U256::from(page_size);
    let list = accounts::list_wallets_in_group(api_key, gid, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_wallets_in_group failed"))?;

    let wallet_items = list
        .iter()
        .map(|m| WalletItem {
            id: m.id.to_string(),
            name: m.name.clone(),
            description: m.description.clone(),
            wallet_address: bytes_to_0x_hex(m.pkp_id.as_bytes()),
        })
        .collect();
    Ok(wallet_items)
}

pub async fn list_actions(
    api_key: &str,
    group_id: Option<&str>,
    page_number: u64,
    page_size: u64,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let pn = U256::from(page_number);
    let ps = U256::from(page_size);
    let list = match group_id {
        Some(gid_str) => {
            let gid = string_group_id_to_u256(gid_str)?;
            if gid == U256::zero() {
                accounts::list_actions(api_key, pn, ps)
                    .await
                    .map_err(|e| ApiStatus::internal_server_error(e, "list_actions failed"))?
            } else {
                accounts::list_actions_in_group(api_key, gid, pn, ps)
                    .await
                    .map_err(|e| ApiStatus::internal_server_error(e, "list_actions failed"))?
            }
        }
        None => accounts::list_actions(api_key, pn, ps)
            .await
            .map_err(|e| ApiStatus::internal_server_error(e, "list_actions failed"))?,
    };

    let list = list.iter().map(action_metadata_to_item).collect();
    Ok(list)
}

pub fn get_chain_config_keys() -> ChainConfigKeysResponse {
    ChainConfigKeysResponse {
        keys: config_key_names(),
    }
}

pub async fn get_chain_info() -> Result<NodeChainConfigResponse, ApiStatus> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or(anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    Ok(NodeChainConfigResponse {
        chain_name: chain_info.chain_name.to_string(),
        chain_id: chain_info.chain_id,
        is_evm: chain_info.is_evm,
        testnet: chain_info.testnet,
        token: chain_info.token.to_string(),
        rpc_url: node_config.chain.rpc_url(),
        contract_address: node_config.contract_address.to_string(),
    })
}

pub async fn get_api_payers() -> Result<Vec<String>, ApiStatus> {
    let mut api_payers = Vec::new();
    let payer_count = accounts::get_api_payer_count().await?;

    for payer_number in 1..=payer_count {
        let api_payer = dstack::v1::get_lit_payer_key(payer_number as u16)
            .await
            .map_err(|e| {
                ApiStatus::internal_server_error(anyhow::anyhow!(e), "get_api_payers failed")
            })?;

        let local_wallet = LocalWallet::from_bytes(&api_payer).map_err(|e| {
            ApiStatus::internal_server_error(anyhow::anyhow!(e), "LocalWallet::from_bytes failed")
        })?;
        let wallet_address = local_wallet.address();
        api_payers.push(bytes_to_0x_hex(wallet_address.as_bytes()));
    }
    Ok(api_payers)
}

pub async fn get_admin_api_payer() -> Result<String, ApiStatus> {
    let admin_api_payer = dstack::v1::get_admin_api_payer_key().await.map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "get_admin_api_payer failed")
    })?;
    let local_wallet = LocalWallet::from_bytes(&admin_api_payer).map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "LocalWallet::from_bytes failed")
    })?;
    let wallet_address = local_wallet.address();
    Ok(bytes_to_0x_hex(wallet_address.as_bytes()))
}
