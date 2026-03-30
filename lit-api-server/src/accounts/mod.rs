pub mod chain_config;
pub mod contracts;
use std::sync::Arc;

pub use contracts::account_config_contract::{AccountConfig, Metadata};
pub mod signable_contract;
pub mod signer_pool;
pub use anyhow::Result;

use crate::accounts::contracts::account_config_contract::{
    KeyValueReturn, PkpData, UsageApiKeyReturn,
};
use crate::accounts::signable_contract::{
    get_read_only_account_config_contract, get_signable_account_config_contract, send_transaction,
};
use crate::accounts::signer_pool::SignerPool;
use crate::core::v1::models::request::{
    AddActionRequest, AddUsageApiKeyRequest, UpdateUsageApiKeyRequest,
};
use crate::utils::parse_with_hash::ipfs_cid_to_u256;
use crate::utils::parse_with_hash::{api_key_hash, usage_api_key_to_hash};
use ethers::types::{Address, H160, U256};
use tracing::instrument;

/// Create a new account. `initial_balance` is stored on the account's apiKey (AccountConfig.accountApiKey.balance).
pub async fn new_account(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    account_name: &str,
    account_description: &str,
    creator_wallet_address: H160,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let api_key_hash = api_key_hash(api_key);

    let function_call = contract.new_account(
        api_key_hash,
        true,
        account_name.to_string(),
        account_description.to_string(),
        creator_wallet_address,
    );
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Check whether an account exists and is mutable. Uses an api_payer address as the
/// simulated caller (msg.sender) because accountExistsAndIsMutable requires the
/// caller to be an api_payer (for managed accounts) or the creator.
pub async fn account_exists(api_key: &str) -> Result<bool> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let api_payers = get_api_payers().await?;
    let from = api_payers.first().copied().ok_or_else(|| {
        anyhow::anyhow!("No api_payers configured; cannot simulate account_exists")
    })?;
    let exists = contract
        .account_exists_and_is_mutable(account_api_key_hash)
        .from(from)
        .call()
        .await?;
    Ok(exists)
}

/// Add a group to an account with name, description, permitted action CID hashes, wallet hashes, and permission flags.
/// `permitted_actions` and `wallets` are keccak256 hashes (U256). Use `keccak256(action_ipfs_cid)` and `keccak256(pkp_public_key)` to produce them.
/// `all_wallets_permitted` and `all_actions_permitted` match AccountConfig.sol Group fields.
#[allow(clippy::too_many_arguments)]
pub async fn add_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    name: &str,
    description: &str,
    cid_hashes: Vec<U256>,
    pkp_ids: Vec<Address>,
) -> Result<U256> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);

    // Simulate the call first to obtain the returned group ID.
    let sim_call = contract.add_group(
        account_api_key_hash,
        name.to_string(),
        description.to_string(),
        cid_hashes.clone(),
        pkp_ids.clone(),
    );
    let group_id = match sim_call.call().await {
        Ok(id) => id,
        Err(e) => {
            // Release the signer back to the pool before propagating.
            signer_pool.release(signer_address).await?;
            return Err(e.into());
        }
    };

    let function_call = contract.add_group(
        account_api_key_hash,
        name.to_string(),
        description.to_string(),
        cid_hashes,
        pkp_ids,
    );
    send_transaction(function_call, signer_pool, signer_address, client).await?;
    Ok(group_id)
}

/// Create a new action entry with name, description, and IPFS CID hash in the account's actionMetadata mapping.
pub async fn add_action(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    action_hash: U256,
    req: AddActionRequest,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call =
        contract.add_action(account_api_key_hash, req.name, req.description, action_hash);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Remove an action from the account by its hash (AccountConfig.removeAction).
pub async fn remove_action(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    action_hash: U256,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.remove_action(account_api_key_hash, action_hash);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Add an action to a group by its IPFS CID. Metadata must be set separately via add_action / update_action_metadata.
pub async fn add_action_to_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
    action_ipfs_cid: &str,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let action_hash = ipfs_cid_to_u256(action_ipfs_cid)
        .map_err(|e| anyhow::anyhow!("Unable to parse action IPFS CID: {}", e))?;
    let function_call = contract.add_action_to_group(account_api_key_hash, group_id, action_hash);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Add a PKP to a group by its address (AccountConfig.addPkpToGroup).
pub async fn add_pkp_to_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
    pkp_id: H160,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.add_pkp_to_group(account_api_key_hash, group_id, pkp_id);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Update group metadata (AccountConfig.updateGroup).
pub async fn update_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
    name: &str,
    description: &str,
    cid_hashes: Vec<U256>,
    pkp_ids: Vec<Address>,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.update_group(
        account_api_key_hash,
        group_id,
        name.to_string(),
        description.to_string(),
        cid_hashes,
        pkp_ids,
    );
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Remove an action from a group by action hash (AccountConfig.removeActionFromGroup). `action_hash` is keccak256 of the action (e.g. IPFS CID).
pub async fn remove_action_from_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
    action_hash: U256,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call =
        contract.remove_action_from_group(account_api_key_hash, group_id, action_hash);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Remove an action from a group by IPFS CID string (hashed with keccak256). Convenience wrapper for remove_action_from_group.
pub async fn remove_action_from_group_by_cid(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
    action_ipfs_cid: &str,
) -> Result<bool> {
    let action_hash = ipfs_cid_to_u256(action_ipfs_cid)
        .map_err(|e| anyhow::anyhow!("Unable to parse action IPFS CID: {}", e))?;
    remove_action_from_group(signer_pool, api_key, group_id, action_hash).await
}

/// Update action metadata (name, description) for an action in a group (AccountConfig.updateActionMetadata). `action_hash` is keccak256 of the action.
pub async fn update_action_metadata(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    action_hash: U256,
    group_id: U256,
    name: &str,
    description: &str,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.update_action_metadata(
        account_api_key_hash,
        action_hash,
        group_id,
        name.to_string(),
        description.to_string(),
    );
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Update usage API key metadata (name, description) (AccountConfig.updateUsageApiKeyMetadata).
pub async fn update_usage_api_key_metadata(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    usage_api_key: &str,
    name: &str,
    description: &str,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = usage_api_key_to_hash(usage_api_key);
    let function_call = contract.update_usage_api_key_metadata(
        account_api_key_hash,
        usage_api_key_hash,
        name.to_string(),
        description.to_string(),
    );
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Remove a PKP from a group by its address (AccountConfig.removePkpFromGroup).
pub async fn remove_pkp_from_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
    pkp_id: H160,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.remove_pkp_from_group(account_api_key_hash, group_id, pkp_id);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Add a usage API key to an account (usageApiKey in AccountConfig.sol).
pub async fn add_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    usage_api_key: &str,
    expiration: U256,
    balance: U256,
    req: AddUsageApiKeyRequest,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    tracing::info!(
        "Adding usage API key to account: {}, usage_api_key: {}, expiration: {}, balance: {}",
        api_key,
        usage_api_key,
        expiration,
        balance
    );
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = api_key_hash(usage_api_key);

    let function_call = contract.set_usage_api_key(
        account_api_key_hash,
        usage_api_key_hash,
        expiration,
        balance,
        req.name.to_string(),
        req.description.to_string(),
        req.can_create_groups,
        req.can_delete_groups,
        req.can_create_pkps,
        req.manage_ipfs_ids_in_groups
            .into_iter()
            .map(U256::from)
            .collect(),
        req.add_pkp_to_groups.into_iter().map(U256::from).collect(),
        req.remove_pkp_from_groups
            .into_iter()
            .map(U256::from)
            .collect(),
        req.execute_in_groups.into_iter().map(U256::from).collect(),
    );
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Update all metadata and permissions on an existing usage API key (AccountConfig.setUsageApiKey).
/// Re-uses the existing key hash — does not create a new wallet or key.
pub async fn update_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    usage_api_key: &str,
    expiration: U256,
    balance: U256,
    req: UpdateUsageApiKeyRequest,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = usage_api_key_to_hash(usage_api_key);
    let function_call = contract.set_usage_api_key(
        account_api_key_hash,
        usage_api_key_hash,
        expiration,
        balance,
        req.name,
        req.description,
        req.can_create_groups,
        req.can_delete_groups,
        req.can_create_pkps,
        req.manage_ipfs_ids_in_groups
            .into_iter()
            .map(U256::from)
            .collect(),
        req.add_pkp_to_groups.into_iter().map(U256::from).collect(),
        req.remove_pkp_from_groups
            .into_iter()
            .map(U256::from)
            .collect(),
        req.execute_in_groups.into_iter().map(U256::from).collect(),
    );
    send_transaction(function_call, signer_pool, signer_address, client.clone()).await
}

/// Remove a usage API key from an account.
pub async fn remove_usage_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    usage_api_key: &str,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = usage_api_key_to_hash(usage_api_key);

    let function_call = contract.remove_usage_api_key(account_api_key_hash, usage_api_key_hash);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Remove a group from an account (AccountConfig.removeGroup).
pub async fn remove_group(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    group_id: U256,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.remove_group(account_api_key_hash, group_id);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Register the derivation path for a wallet address under an account (AccountConfig.wallet_derivation).
/// `wallet_address` is the hex address (with or without 0x). `derivation_path` is the path stored on-chain.
pub async fn register_wallet_derivation(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    wallet_address: H160,
    derivation_path: U256,
    name: &str,
    description: &str,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.register_wallet_derivation(
        account_api_key_hash,
        wallet_address,
        derivation_path,
        name.to_string(),
        description.to_string(),
    );

    send_transaction(function_call, signer_pool, signer_address, client).await
}

/// Get the derivation path for a wallet address under an account (read-only).
/// `wallet_address` is the hex address (with or without 0x). Returns the U256 derivation path, or errors if not set.
#[instrument(
    name = "accounts::get_wallet_derivation",
    level = "debug",
    skip_all,
    err
)]
pub async fn get_wallet_derivation(api_key: &str, wallet_address: H160) -> Result<U256> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let derivation = contract
        .get_wallet_derivation(account_api_key_hash, wallet_address)
        .call()
        .await?;
    Ok(derivation)
}

/// List groups for an account (paginated). Returns metadata (id, name, description) per group.
pub async fn list_groups(
    api_key: &str,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<Metadata>> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_groups(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List PKPs (wallet derivation metadata) for an account (paginated).
pub async fn list_wallets(
    api_key: &str,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<PkpData>> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);

    let page = contract
        .list_pkps(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List PKPs in a group (paginated). Returns metadata for each PKP in the group.
pub async fn list_wallets_in_group(
    api_key: &str,
    group_id: U256,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<PkpData>> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);

    let page = contract
        .list_wallets_in_group(account_api_key_hash, group_id, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List all actions on the account (paginated). Returns metadata (id, name, description) per action.
pub async fn list_actions(
    api_key: &str,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<Metadata>> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_actions(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List actions in a group (paginated). Returns metadata (id, name, description) per action.
pub async fn list_actions_in_group(
    api_key: &str,
    group_id: U256,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<Metadata>> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_actions_in_group(account_api_key_hash, group_id, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List usage API keys for an account (paginated). Returns metadata (id, name, description) per usage API key.
pub async fn list_api_keys(
    api_key: &str,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<UsageApiKeyReturn>> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_api_keys(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

pub async fn debit_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    amount: U256,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.debit_api_key(account_api_key_hash, amount);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

pub async fn credit_api_key(
    signer_pool: Arc<SignerPool>,
    api_key: &str,
    amount: U256,
) -> Result<bool> {
    let (contract, signer_address, client) =
        get_signable_account_config_contract(signer_pool.clone()).await?;

    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.credit_api_key(account_api_key_hash, amount);
    send_transaction(function_call, signer_pool, signer_address, client).await
}

pub async fn get_api_payers() -> Result<Vec<H160>> {
    let contract = get_read_only_account_config_contract().await?;
    let api_payers = contract.api_payers().call().await?;
    Ok(api_payers)
}

pub async fn get_requested_api_payer_count() -> Result<usize> {
    let contract = get_read_only_account_config_contract().await?;
    let requested_signer_count = contract.requested_api_payer_count().call().await?;
    Ok(requested_signer_count.as_usize())
}

pub async fn get_api_payer_count() -> Result<usize> {
    let contract = get_read_only_account_config_contract().await?;
    let signer_count = contract.api_payer_count().call().await?;
    Ok(signer_count.as_usize())
}

pub async fn get_rebalance_amount() -> Result<U256> {
    let contract = get_read_only_account_config_contract().await?;
    let rebalance_amount = contract.rebalance_amount().call().await?;
    Ok(rebalance_amount)
}

#[instrument(name = "accounts::can_execute_action", level = "debug", skip_all, err)]
pub async fn can_execute_action(api_key: &str, cid_hash: U256) -> Result<bool> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let can_execute = contract
        .can_execute_action(account_api_key_hash, cid_hash)
        .call()
        .await?;
    Ok(can_execute)
}

#[instrument(
    name = "accounts::can_use_wallet_in_action",
    level = "debug",
    skip_all,
    err
)]
pub async fn can_use_wallet_in_action(
    api_key: &str,
    cid_hash: U256,
    wallet_address: H160,
) -> Result<bool> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let can_use = contract
        .can_use_wallet_in_action(account_api_key_hash, cid_hash, wallet_address)
        .call()
        .await?;
    Ok(can_use)
}

pub async fn can_execute_action_and_use_wallet(
    api_key: &str,
    cid_hash: U256,
    wallet_address: H160,
) -> Result<(bool, bool)> {
    let contract = get_read_only_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let result = contract
        .can_execute_action_and_use_wallet(account_api_key_hash, cid_hash, wallet_address)
        .call()
        .await?;
    Ok(result)
}

pub async fn get_node_configuration_values() -> Result<Vec<KeyValueReturn>> {
    let contract = get_read_only_account_config_contract().await?;
    let values = contract.node_configuration_values().call().await?;
    Ok(values)
}
