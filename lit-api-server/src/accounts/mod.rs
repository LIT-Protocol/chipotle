pub mod contracts;
pub use contracts::account_config::{AccountConfig, Metadata};
pub mod signable_contract;
pub use anyhow::Result;

use crate::accounts::contracts::account_config::UsageApiKey;
use crate::accounts::signable_contract::get_signable_account_config_contract;
use crate::core::lookup_data;
use ethers::types::{H160, U256};
use ethers::utils::keccak256;
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};

pub fn api_key_hash(api_key_base_64: &str) -> U256 {
    U256::from_big_endian(&keccak256(api_key_base_64.as_bytes()))
}

/// keccak256 of wallet address bytes (hex string with or without 0x) as U256.
pub fn wallet_address_hash(wallet_address: H160) -> U256 {
    U256::from_big_endian(&keccak256(wallet_address.as_bytes()))
}

pub fn derivation_path(wallet_address: H160) -> U256 {
    U256::from_big_endian(&keccak256(wallet_address.as_bytes()))
}

pub fn address_from_pubkey(pubkey: &str) -> Result<H160> {
    let pubkey_bytes = hex_to_bytes(pubkey)?;
    address_from_pubkey_bytes(&pubkey_bytes)
}

pub fn address_from_pubkey_bytes(pubkey_bytes: &[u8]) -> Result<H160> {
    let address = H160::from_slice(&keccak256(pubkey_bytes)[12..]);
    Ok(address)
}

/// Create a new account. `initial_balance` is stored on the account's apiKey (AccountConfig.accountApiKey.balance).
pub async fn new_account(
    api_key: &str,
    account_name: &str,
    account_description: &str,
    creator_wallet_address: H160,
    initial_balance: U256,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let api_key_hash = api_key_hash(api_key);
    lookup_data::add_api_key(&api_key_hash.to_string(), api_key).await?;

    let function_call = contract.new_account(
        api_key_hash,
        true,
        account_name.to_string(),
        account_description.to_string(),
        creator_wallet_address,
        initial_balance,
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

pub async fn account_exists(api_key: &str) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let exists = contract
        .account_exists_and_is_mutable(account_api_key_hash)
        .call()
        .await?;
    Ok(exists)
}

/// Add a group to an account with name, description, permitted action CID hashes, wallet hashes, and permission flags.
/// `permitted_actions` and `wallets` are keccak256 hashes (U256). Use `keccak256(action_ipfs_cid)` and `keccak256(pkp_public_key)` to produce them.
/// `all_wallets_permitted` and `all_actions_permitted` match AccountConfig.sol Group fields.
pub async fn add_group(
    api_key: &str,
    name: &str,
    description: &str,
    permitted_actions: Vec<U256>,
    wallets: Vec<U256>,
    all_wallets_permitted: bool,
    all_actions_permitted: bool,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.add_group(
        account_api_key_hash,
        name.to_string(),
        description.to_string(),
        permitted_actions,
        wallets,
        all_wallets_permitted,
        all_actions_permitted,
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Add an action (IPFS CID) to a group with optional metadata. `action_ipfs_cid` is hashed with keccak256; pass the raw CID string.
pub async fn add_action_to_group(
    api_key: &str,
    group_id: U256,
    action_ipfs_cid: &str,
    name: &str,
    description: &str,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let action_hash = U256::from_big_endian(&keccak256(action_ipfs_cid));
    let function_call = contract.add_action_to_group(
        account_api_key_hash,
        group_id,
        action_hash,
        name.to_string(),
        description.to_string(),
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Add a wallet (by address hash) to a group. `wallet_address` is hashed with keccak256 (hex with or without 0x).
pub async fn add_wallet_to_group(
    api_key: &str,
    group_id: U256,
    wallet_address: H160,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let wallet_address_hash = wallet_address_hash(wallet_address);
    let function_call =
        contract.add_wallet_to_group(account_api_key_hash, group_id, wallet_address_hash);
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Add a PKP to a group (alias for add_wallet_to_group; hashes the given string and adds to group).
pub async fn add_pkp_to_group(api_key: &str, group_id: U256, wallet_address: H160) -> Result<bool> {
    add_wallet_to_group(api_key, group_id, wallet_address).await
}

/// Update group metadata and permission flags (AccountConfig.updateGroup).
pub async fn update_group(
    api_key: &str,
    group_id: U256,
    name: &str,
    description: &str,
    all_wallets_permitted: bool,
    all_actions_permitted: bool,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.update_group(
        account_api_key_hash,
        group_id,
        name.to_string(),
        description.to_string(),
        all_wallets_permitted,
        all_actions_permitted,
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Remove an action from a group by action hash (AccountConfig.removeActionFromGroup). `action_hash` is keccak256 of the action (e.g. IPFS CID).
pub async fn remove_action_from_group(
    api_key: &str,
    group_id: U256,
    action_hash: U256,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call =
        contract.remove_action_from_group(account_api_key_hash, group_id, action_hash);
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Remove an action from a group by IPFS CID string (hashed with keccak256). Convenience wrapper for remove_action_from_group.
pub async fn remove_action_from_group_by_cid(
    api_key: &str,
    group_id: U256,
    action_ipfs_cid: &str,
) -> Result<bool> {
    let action_hash = U256::from_big_endian(&keccak256(action_ipfs_cid));
    remove_action_from_group(api_key, group_id, action_hash).await
}

/// Update action metadata (name, description) for an action in a group (AccountConfig.updateActionMetadata). `action_hash` is keccak256 of the action.
pub async fn update_action_metadata(
    api_key: &str,
    action_hash: U256,
    group_id: U256,
    name: &str,
    description: &str,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.update_action_metadata(
        account_api_key_hash,
        action_hash,
        group_id,
        name.to_string(),
        description.to_string(),
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Update usage API key metadata (name, description) (AccountConfig.updateUsageApiKeyMetadata).
pub async fn update_usage_api_key_metadata(
    api_key: &str,
    usage_api_key: &str,
    name: &str,
    description: &str,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = U256::from_big_endian(&keccak256(usage_api_key));
    let function_call = contract.update_usage_api_key_metadata(
        account_api_key_hash,
        usage_api_key_hash,
        name.to_string(),
        description.to_string(),
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Remove a wallet from a group. `wallet_address` must match the value used when adding (same keccak256 input).
pub async fn remove_wallet_from_group(
    api_key: &str,
    group_id: U256,
    wallet_address: H160,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let wallet_address_hash = wallet_address_hash(wallet_address);
    let function_call =
        contract.remove_wallet_from_group(account_api_key_hash, group_id, wallet_address_hash);
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Remove a PKP from a group (alias for remove_wallet_from_group).
pub async fn remove_pkp_from_group(
    api_key: &str,
    group_id: U256,
    wallet_address: H160,
) -> Result<bool> {
    remove_wallet_from_group(api_key, group_id, wallet_address).await
}

/// Add a usage API key to an account (usageApiKey in AccountConfig.sol).
pub async fn add_usage_api_key(
    api_key: &str,
    usage_api_key: &str,
    expiration: U256,
    balance: U256,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    tracing::info!(
        "Adding usage API key to account: {}, usage_api_key: {}, expiration: {}, balance: {}",
        api_key,
        usage_api_key,
        expiration,
        balance
    );
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = api_key_hash(usage_api_key);
    lookup_data::add_api_key(&usage_api_key_hash.to_string(), usage_api_key).await?;
    let function_call = contract.add_api_key(
        account_api_key_hash,
        usage_api_key_hash,
        expiration,
        balance,
    );
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Remove a usage API key from an account.
pub async fn remove_usage_api_key(api_key: &str, usage_api_key: &str) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let usage_api_key_hash = api_key_hash(usage_api_key);
    let usage_api_key_hash_string = bytes_to_hex(keccak256(usage_api_key.as_bytes()));
    lookup_data::delete_api_key_by_key_hash(&usage_api_key_hash_string).await?;
    let function_call = contract.remove_usage_api_key(account_api_key_hash, usage_api_key_hash);
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Register the derivation path for a wallet address under an account (AccountConfig.wallet_derivation).
/// `wallet_address` is the hex address (with or without 0x). `derivation_path` is the path stored on-chain.
pub async fn register_wallet_derivation(
    api_key: &str,
    wallet_address: H160,
    derivation_path: U256,
    name: &str,
    description: &str,
) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let wallet_address_hash = wallet_address_hash(wallet_address);
    let function_call = contract.register_wallet_derivation(
        account_api_key_hash,
        wallet_address_hash,
        derivation_path,
        name.to_string(),
        description.to_string(),
    );

    let tx = function_call.send().await?;
    // Wait for the transaction to be confirmed before returning true
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

pub async fn get_wallet_derivation_from_pubkey(api_key: &str, pubkey: &str) -> Result<U256> {
    let wallet_address = address_from_pubkey(pubkey)?;
    get_wallet_derivation(api_key, wallet_address).await
}

/// Get the derivation path for a wallet address under an account (read-only).
/// `wallet_address` is the hex address (with or without 0x). Returns the U256 derivation path, or errors if not set.
pub async fn get_wallet_derivation(api_key: &str, wallet_address: H160) -> Result<U256> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let wallet_address_hash = wallet_address_hash(wallet_address);
    let derivation = contract
        .get_wallet_derivation(account_api_key_hash, wallet_address_hash)
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
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_groups(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List wallets (wallet derivation metadata) for an account (paginated).
pub async fn list_wallets(
    api_key: &str,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<Metadata>> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_wallets(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List wallets in a group (paginated). Returns metadata for each wallet in the group.
pub async fn list_wallets_in_group(
    api_key: &str,
    group_id: U256,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<Metadata>> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_wallets_in_group(account_api_key_hash, group_id, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List actions in a group (paginated). Returns metadata (id, name, description) per action.
pub async fn list_actions(
    api_key: &str,
    group_id: U256,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<Metadata>> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_actions(account_api_key_hash, group_id, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

/// List usage API keys for an account (paginated). Returns metadata (id, name, description) per usage API key.
pub async fn list_api_keys(
    api_key: &str,
    page_number: U256,
    page_size: U256,
) -> Result<Vec<UsageApiKey>> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let page = contract
        .list_api_keys(account_api_key_hash, page_number, page_size)
        .call()
        .await?;
    Ok(page)
}

pub async fn debit_api_key(api_key: &str, amount: U256) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.debit_api_key(account_api_key_hash, amount);
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

pub async fn credit_api_key(api_key: &str, amount: U256) -> Result<bool> {
    let contract = get_signable_account_config_contract().await?;
    let account_api_key_hash = api_key_hash(api_key);
    let function_call = contract.credit_api_key(account_api_key_hash, amount);
    let tx = function_call.send().await?;
    match tx.await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}
