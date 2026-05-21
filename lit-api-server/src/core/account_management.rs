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
    is_precomputed_hash_shape, string_group_id_to_u256,
};
use crate::{accounts, dstack};
use alloy::primitives::{Address, U256};
use alloy::signers::local::PrivateKeySigner;
use elliptic_curve::group::GroupEncoding;
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

/// Encode a 32-byte secret into the base64 form used for raw API keys, and
/// reject anything shaped like a precomputed account hash (CPL-285).
///
/// Standard base64 of 32 bytes is 44 chars, so the 66-char hash shape is
/// arithmetically unreachable today. The check is defense-in-depth against a
/// future format change (e.g. switching to 0x-prefixed hex) that would create a
/// confused-deputy collision with `usage_api_key_to_hash`. `debug_assert!`
/// fires in dev/test, the runtime branch returns 500 in release.
fn encode_api_key_from_secret(secret: &[u8; 32]) -> Result<String, ApiStatus> {
    let encoded = base64_light::base64_encode_bytes(secret);
    debug_assert!(
        !is_precomputed_hash_shape(&encoded),
        "API key generator produced a value shaped like a precomputed account hash; \
         this would collide with usage_api_key_to_hash and route to the wrong on-chain account"
    );
    if is_precomputed_hash_shape(&encoded) {
        return Err(ApiStatus::internal_server_error(
            anyhow::anyhow!("generated api_key matched precomputed-hash shape"),
            "Internal key generation invariant violated",
        ));
    }
    Ok(encoded)
}

// Create a new wallet and return the public key, wallet address, and secret.
async fn create_new_wallet() -> Result<(String, Address, [u8; 32], U256), ApiStatus> {
    let (derivation_u256, derivation_path) = generate_unique_derivation_path();
    tracing::info!(
        "Creating new wallet with derivation path: {}",
        derivation_path
    );
    let secret: [u8; 32] = get_client_key(&derivation_path).await.map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "get_client_key failed")
    })?;

    let signer = PrivateKeySigner::from_slice(&secret).map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "PrivateKeySigner::from_slice failed")
    })?;
    let wallet_address = signer.address();
    let public_key_bytes = signer.credential().verifying_key().as_affine().to_bytes();
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
    let api_key = encode_api_key_from_secret(&secret)?;

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
        let wallet_hex = bytes_to_0x_hex(wallet_address.as_slice());
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
        wallet_address: bytes_to_0x_hex(wallet_address.as_slice()),
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
        wallet_address: bytes_to_0x_hex(wallet_address.as_slice()),
    })
}

pub async fn create_wallet_with_signature(
    req: Json<CreateWalletWithSignatureRequest>,
) -> Result<CreateWalletWithSignatureResponse, ApiStatus> {
    let signer = crate::core::eip712::verify_eip712_signature(
        &req.typed_data,
        &req.signature,
        crate::core::eip712::PRIMARY_TYPE_CREATE_WALLET,
    )?;
    tracing::info!(
        "create_wallet_with_signature: minting PKP for ChainSecured signer {:?}",
        signer
    );
    let (_public_key, wallet_address, _secret, derivation_u256) = create_new_wallet().await?;
    Ok(CreateWalletWithSignatureResponse {
        wallet_address: bytes_to_0x_hex(wallet_address.as_slice()),
        derivation_path: format!("0x{:x}", derivation_u256),
    })
}

/// ChainSecured usage API key: server only mints the wallet (PKP) gated by an
/// EIP-712 wallet signature (`primaryType: "AddUsageApiKey"`); the client
/// follows up with on-chain `registerWalletDerivation` and `setUsageApiKey`
/// signed by their admin wallet (api_payer cannot call setUsageApiKey for
/// ChainSecured accounts — see AppStorage.accountExistsAndIsMutable).
pub async fn add_usage_api_key_with_signature(
    req: Json<AddUsageApiKeyWithSignatureRequest>,
) -> Result<AddUsageApiKeyWithSignatureResponse, ApiStatus> {
    let signer = crate::core::eip712::verify_eip712_signature(
        &req.typed_data,
        &req.signature,
        crate::core::eip712::PRIMARY_TYPE_ADD_USAGE_API_KEY,
    )?;
    tracing::info!(
        "add_usage_api_key_with_signature: minting usage-key PKP for ChainSecured signer {:?}",
        signer
    );
    let (_public_key, wallet_address, secret, derivation_u256) = create_new_wallet().await?;
    let usage_api_key = encode_api_key_from_secret(&secret)?;
    Ok(AddUsageApiKeyWithSignatureResponse {
        usage_api_key,
        wallet_address: bytes_to_0x_hex(wallet_address.as_slice()),
        derivation_path: format!("0x{:x}", derivation_u256),
    })
}

/// Convert a managed (API-mode) account into a ChainSecured (sovereign) account by
/// reassigning its admin wallet to a user-controlled address. The user proves
/// ownership of `new_admin_wallet_address` with an EIP-712 typed-data signature
/// (`primaryType: "ConvertAccount"`). The api_payer signs the on-chain
/// `convertToChainSecuredAccount` call.
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
    let claimed_address = Address::from_slice(&claimed_address_bytes);
    if claimed_address == Address::ZERO {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("new_admin_wallet_address must be non-zero"),
            "new_admin_wallet_address must be non-zero",
        ));
    }

    let signer = crate::core::eip712::verify_eip712_signature(
        &req.typed_data,
        &req.signature,
        crate::core::eip712::PRIMARY_TYPE_CONVERT_ACCOUNT,
    )?;
    if signer != claimed_address {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Signature does not match new_admin_wallet_address"),
            "Signature does not match new_admin_wallet_address",
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
    if action_hash == U256::ZERO {
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
    let wallet_address = Address::from_slice(&wallet_address_bytes);
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
    let wallet_address = Address::from_slice(&src);
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

    let usage_api_key = encode_api_key_from_secret(&secret)?;

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
    if group_id == U256::ZERO {
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
    if group_id == U256::ZERO {
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
    if group_id == U256::ZERO {
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
    if action_hash == U256::ZERO {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("Cannot update action with hash 0x0"),
            "Cannot update action with hash 0x0",
        ));
    }
    accounts::update_action_metadata(
        signer_pool,
        api_key,
        action_hash,
        U256::ZERO,
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
    // `accounts::Metadata` is still an ethers-generated struct (Phase 5 will
    // regenerate it via sol!); compare against ethers' zero until then.
    if m.id == ethers::types::U256::zero() {
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
            if gid == U256::ZERO {
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

        let signer = PrivateKeySigner::from_slice(&api_payer).map_err(|e| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!(e),
                "PrivateKeySigner::from_slice failed",
            )
        })?;
        api_payers.push(bytes_to_0x_hex(signer.address().as_slice()));
    }
    Ok(api_payers)
}

pub async fn get_admin_api_payer() -> Result<String, ApiStatus> {
    let admin_api_payer = dstack::v1::get_admin_api_payer_key().await.map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "get_admin_api_payer failed")
    })?;
    let signer = PrivateKeySigner::from_slice(&admin_api_payer).map_err(|e| {
        ApiStatus::internal_server_error(anyhow::anyhow!(e), "PrivateKeySigner::from_slice failed")
    })?;
    Ok(bytes_to_0x_hex(signer.address().as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base64 of 32 bytes is always 44 chars, so it can never collide with the
    /// 66-char `0x[hex]{64}` precomputed-hash shape (CPL-285). This test pins
    /// that property so a future change to the encoding is forced to confront it.
    #[test]
    fn encode_api_key_from_secret_never_matches_hash_shape() {
        // Sample a few representative byte patterns: zeros, all-ones, a hash-like
        // pattern, and a typical random secret.
        for secret in &[
            [0u8; 32],
            [0xffu8; 32],
            // hex digits 0-9 a-f then repeated
            [
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
                0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
                0x89, 0xab, 0xcd, 0xef,
            ],
            [
                0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
                0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x12, 0x34, 0x56, 0x78,
                0x9a, 0xbc, 0xde, 0xf0,
            ],
        ] {
            let encoded = encode_api_key_from_secret(secret).expect("happy path must succeed");
            assert_eq!(encoded.len(), 44, "base64 of 32 bytes is always 44 chars");
            assert!(
                !crate::utils::parse_with_hash::is_precomputed_hash_shape(&encoded),
                "encoded API key {encoded:?} unexpectedly matched precomputed-hash shape",
            );
        }
    }
}
