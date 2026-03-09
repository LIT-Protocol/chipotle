use std::sync::Arc;

use crate::accounts::signer_pool::SignerPool;
use crate::config::GLOBAL_NODE_CONFIG;
use crate::core::api_status::ApiStatus;
use crate::core::v1::models::request::{
    AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest, AddUsageApiKeyRequest,
    NewAccountRequest, RemoveActionFromGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest,
};
use crate::core::v1::models::response::{
    AccountOpResponse, AddUsageApiKeyResponse, ApiKeyItem, CreateWalletResponse, ListMetadataItem,
    NewAccountResponse, NodeChainConfigResponse, WalletItem,
};
use crate::dstack::v1::get_client_key;
use crate::{accounts, dstack};
use elliptic_curve::group::GroupEncoding;
use ethers::signers::{LocalWallet, Signer};
use ethers::types::{H160, U256};
use ethers::utils::keccak256;
use ipfs_hasher::IpfsHasher;
use lit_core::utils::binary::{bytes_to_0x_hex, bytes_to_hex, hex_to_bytes};
use rand::Rng;
use rocket::serde::json::Json;
/// Parse U256 from decimal string or hex string (with or without 0x prefix).
fn parse_u256(s: &str) -> Result<U256, ApiStatus> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        let bytes = hex_to_bytes(s)
            .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex for U256"))?;
        Ok(U256::from_big_endian(&bytes))
    } else {
        U256::from_dec_str(s)
            .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid decimal for U256"))
    }
}

/// Parse vec of hex strings to Vec<U256> (for permitted_actions / pkps hashes).
fn parse_u256_hex_list(strings: &[String]) -> Result<Vec<U256>, ApiStatus> {
    strings
        .iter()
        .map(|s| {
            let bytes = hex_to_bytes(s.trim())
                .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex in list"))?;
            Ok(U256::from_big_endian(&bytes))
        })
        .collect::<Result<Vec<_>, _>>()
}

fn get_random_secret() -> [u8; 32] {
    let mut secret: [u8; 32] = [0; 32];
    // Get a thread-local random number generator and fill the array.
    rand::thread_rng().fill(&mut secret);
    secret
}

// Generate a unique derivation path for a new wallet.
fn generate_unique_derivation_path() -> Result<(U256, String), ApiStatus> {
    let seed_bytes = get_random_secret();
    let derivation_bytes = keccak256(seed_bytes);
    let derivation_u256 = U256::from_big_endian(&derivation_bytes);
    let derivation_path = bytes_to_hex(derivation_bytes);
    Ok((derivation_u256, derivation_path))
}

// Create a new wallet and return the public key, wallet address, and secret.
async fn create_new_wallet() -> Result<(String, H160, [u8; 32], U256), ApiStatus> {
    let (derivation_u256, derivation_path) = generate_unique_derivation_path()?;
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
    new_account_request: Json<NewAccountRequest>,
) -> Result<NewAccountResponse, ApiStatus> {
    let account_name = new_account_request.account_name.clone();
    let account_description = new_account_request.account_description.clone();

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

    // technically this is NOT a derivaton path at all, but it's a stand-in for now
    accounts::register_wallet_derivation(
        signer_pool,
        api_key,
        wallet_address,
        derivation_u256,
        "Wallet",
        "Wallet",
    )
    .await?;

    Ok(CreateWalletResponse {
        wallet_address: bytes_to_0x_hex(wallet_address.as_bytes()),
    })
}

pub async fn get_lit_action_ipfs_id(code: String) -> Result<String, ApiStatus> {
    let ipfs_hasher = IpfsHasher::default();
    let derived_ipfs_id = ipfs_hasher.compute(code.as_bytes());
    Ok(derived_ipfs_id)
}

pub async fn add_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let permitted_actions = parse_u256_hex_list(&req.permitted_actions)?;
    let pkps = parse_u256_hex_list(&req.pkps)?;
    accounts::add_group(
        signer_pool,
        api_key,
        &req.group_name,
        &req.group_description,
        permitted_actions,
        pkps,
        req.all_wallets_permitted,
        req.all_actions_permitted,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "add_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_action_to_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddActionToGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
    let name = req.name.as_deref().unwrap_or("");
    let description = req.description.as_deref().unwrap_or("");
    accounts::add_action_to_group(
        signer_pool,
        api_key,
        group_id,
        &req.action_ipfs_cid,
        name,
        description,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "add_action_to_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_pkp_to_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddPkpToGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
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
        .map_err(|e| ApiStatus::internal_server_error(e, "add_pkp_to_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn remove_pkp_from_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<RemovePkpFromGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
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
        .map_err(|e| ApiStatus::internal_server_error(e, "remove_pkp_from_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<AddUsageApiKeyRequest>,
) -> Result<AddUsageApiKeyResponse, ApiStatus> {
    let expiration = parse_u256(&req.expiration)?;
    let balance = parse_u256(&req.balance)?;

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
        &req.name,
        &req.description,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "add_usage_api_key failed"))?;
    Ok(AddUsageApiKeyResponse {
        success: true,
        usage_api_key,
    })
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

pub async fn update_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<UpdateGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
    accounts::update_group(
        signer_pool,
        api_key,
        group_id,
        &req.name,
        &req.description,
        req.all_wallets_permitted,
        req.all_actions_permitted,
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
    let group_id = parse_u256(&req.group_id)?;
    accounts::remove_action_from_group_by_cid(signer_pool, api_key, group_id, &req.action_ipfs_cid)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "remove_action_from_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn update_action_metadata(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    req: Json<UpdateActionMetadataRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
    let action_hash = U256::from_big_endian(&keccak256(&req.action_ipfs_cid));
    accounts::update_action_metadata(
        signer_pool,
        api_key,
        action_hash,
        group_id,
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

fn metadata_to_item(m: &accounts::Metadata) -> ListMetadataItem {
    let mut bytes = [0; 32];
    m.id.to_big_endian(&mut bytes);
    ListMetadataItem {
        id: bytes_to_hex(bytes),
        name: m.name.clone(),
        description: m.description.clone(),
    }
}

#[allow(dead_code)]
fn usage_api_key_to_api_key_item(
    m: &accounts::contracts::account_config_contract::UsageApiKey,
) -> ApiKeyItem {
    let mut bytes = [0; 32];
    m.metadata.id.to_big_endian(&mut bytes);
    let id = bytes_to_hex(bytes);

    ApiKeyItem {
        id,
        name: m.metadata.name.clone(),
        description: m.metadata.description.clone(),
        expiration: m.expiration.to_string(),
        balance: m.balance.as_u64(),
    }
}

pub async fn list_api_keys(
    api_key: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ApiKeyItem>, ApiStatus> {
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_api_keys(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_api_keys failed"))?;

    let api_key_items = list
        .iter()
        .map(|m| ApiKeyItem {
            id: m.api_key_hash.to_string(),
            name: m.metadata.name.clone(),
            description: m.metadata.description.clone(),
            expiration: m.expiration.to_string(),
            balance: m.balance.as_u64(),
        })
        .collect();
    Ok(api_key_items)
}

pub async fn list_groups(
    api_key: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_groups(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_groups failed"))?;
    Ok(list.iter().map(metadata_to_item).collect())
}

pub async fn list_wallets(
    api_key: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<WalletItem>, ApiStatus> {
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_wallets(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_wallets failed"))?;

    let wallet_items = list
        .iter()
        .map(|m| WalletItem {
            id: m.id.to_string(),
            name: m.name.clone(),
            description: m.description.clone(),
            wallet_address: bytes_to_hex(m.wallet_address.as_bytes()),
        })
        .collect();
    Ok(wallet_items)
}

pub async fn list_wallets_in_group(
    api_key: &str,
    group_id: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<WalletItem>, ApiStatus> {
    let gid = parse_u256(group_id)?;
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_wallets_in_group(api_key, gid, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_wallets_in_group failed"))?;

    let wallet_items = list
        .iter()
        .map(|m| WalletItem {
            id: m.id.to_string(),
            name: m.name.clone(),
            description: m.description.clone(),
            wallet_address: bytes_to_hex(m.wallet_address.as_bytes()),
        })
        .collect();
    Ok(wallet_items)
}

pub async fn list_actions(
    api_key: &str,
    group_id: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let gid = parse_u256(group_id)?;
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_actions(api_key, gid, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_actions failed"))?;
    Ok(list.iter().map(metadata_to_item).collect())
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
        rpc_url: chain_info.rpc_url.to_string(),
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
