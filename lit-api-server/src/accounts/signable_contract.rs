pub use crate::accounts::contracts::account_config_contract::AccountConfig;
use crate::accounts::decode_revert::decode_contract_revert;
use crate::accounts::signer_pool::SignerPool;
use crate::config::GLOBAL_NODE_CONFIG;
pub use crate::utils::chain_info::Chain;
pub use alloy::contract::CallBuilder;
pub use alloy::network::{Ethereum, TransactionBuilder, TxSigner};
pub use alloy::primitives::{Address, B256};
pub use alloy::providers::{DynProvider, Provider, ProviderBuilder};
pub use alloy::rpc::types::BlockNumberOrTag;
pub use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
pub use alloy::signers::local::PrivateKeySigner;
pub use anyhow::Result;
pub use lit_core::utils::binary::hex_to_bytes;
use std::sync::OnceLock;

/// The shared signing client. A single instance is held for the lifetime of
/// the process so Alloy's recommended fillers (including `NonceFiller`) can
/// manage nonces across concurrent requests. Signer-pool leasing still serializes
/// normal use per payer, while the oldest-lease fallback remains non-blocking.
pub(crate) type SigningClient = DynProvider<Ethereum>;
pub(crate) type AccountConfigInstance = AccountConfig::AccountConfigInstance<SigningClient>;

static GLOBAL_READ_ONLY_CLIENT: OnceLock<SigningClient> = OnceLock::new();

fn rpc_url() -> Result<url::Url> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    Ok(node_config.chain.rpc_url().parse()?)
}

fn read_only_provider() -> Result<SigningClient> {
    Ok(ProviderBuilder::new().connect_http(rpc_url()?).erased())
}

pub(crate) fn signer_provider(wallet: PrivateKeySigner) -> Result<SigningClient> {
    Ok(ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url()?)
        .erased())
}

/// Initialise the global read-only client. Must be called once at startup,
/// after `init_config()`, before account contract access.
pub async fn init_chain_clients() -> Result<()> {
    let provider = read_only_provider()?;
    GLOBAL_READ_ONLY_CLIENT.get_or_init(|| provider);
    Ok(())
}

pub(crate) async fn get_signable_account_config_contract(
    signer_pool: std::sync::Arc<SignerPool>,
) -> Result<(AccountConfigInstance, Address, SigningClient), anyhow::Error> {
    let signer_handle = signer_pool.request().await?;
    let client = signer_handle
        .client
        .ok_or(anyhow::anyhow!("No signer available"))?;
    let signer_address = signer_handle.address;
    let contract = get_account_config_contract(client.clone()).await?;

    Ok((contract, signer_address, client))
}

pub async fn get_account_config_contract(client: SigningClient) -> Result<AccountConfigInstance> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let account_config_address = hex_to_bytes(&node_config.contract_address)?;
    let account_config_address = Address::from_slice(&account_config_address);
    let contract = AccountConfig::new(account_config_address, client);
    Ok(contract)
}

pub async fn get_admin_api_payer_contract() -> Result<AccountConfigInstance> {
    let admin_signer = get_admin_api_signer().await?;
    let contract = get_account_config_contract(admin_signer).await?;
    Ok(contract)
}

pub async fn get_admin_api_signer() -> Result<SigningClient> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();
    let secret = crate::dstack::v1::get_admin_api_payer_key()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get admin api payer key: {e}"))?;
    let wallet = PrivateKeySigner::from_bytes(&B256::from_slice(&secret))?
        .with_chain_id(Some(chain_info.chain_id));

    signer_provider(wallet)
}

/// Read-only provider + the AccountConfig address, for ad-hoc scoped `sol!`
/// interfaces that target functions not yet present in the regenerated giant
/// binding (e.g. the spending-rules view from lambda-parity PR 3). Tracks the
/// workspace alloy version directly; fold callers into the generated binding
/// once it is regenerated on the canonical toolchain.
pub(crate) fn read_only_client_and_address() -> Result<(SigningClient, Address)> {
    let client = GLOBAL_READ_ONLY_CLIENT
        .get()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Read-only client not initialised — call init_chain_clients() at startup"
            )
        })?
        .clone();
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let account_config_address =
        Address::from_slice(&hex_to_bytes(&node_config.contract_address)?);
    Ok((client, account_config_address))
}

pub(crate) async fn get_read_only_account_config_contract() -> Result<AccountConfigInstance> {
    let client = GLOBAL_READ_ONLY_CLIENT
        .get()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Read-only client not initialised — call init_chain_clients() at startup"
            )
        })?
        .clone();

    let contract = get_account_config_contract(client).await?;
    Ok(contract)
}

pub async fn send_transaction<D>(
    function_call: CallBuilder<&SigningClient, D, Ethereum>,
    signer_pool: std::sync::Arc<SignerPool>,
    signer_address: Address,
    client: SigningClient,
) -> Result<bool>
where
    D: alloy::contract::CallDecoder,
{
    // Call-before-send: dry-run via eth_call so any revert surfaces as a
    // decoded, human-readable error before we broadcast. No nonce is consumed
    // and no gas is spent on a failed simulation.
    if let Err(sim_err) = function_call.call().await {
        let decoded = decode_contract_revert(&sim_err);
        if let Err(release_err) = signer_pool.release(signer_address).await {
            tracing::warn!("signer release after sim failure failed: {release_err}");
        }
        return Err(anyhow::anyhow!("Simulation failed: {decoded}"));
    }

    let first_err = match function_call.send().await {
        Ok(tx) => {
            let result = match tx.get_receipt().await {
                Ok(_) => Ok(true),
                Err(e) => Err(anyhow::Error::from(e)),
            };
            signer_pool.release(signer_address).await?;
            return result;
        }
        Err(e) => e,
    };

    let is_nonce_too_low = |err: &dyn std::error::Error| -> bool {
        let mut current = err;
        loop {
            let msg = current.to_string();
            if msg.contains("nonce too low")
                || msg.contains("transaction nonce is too low")
                || msg.contains("replacement transaction underpriced")
            {
                return true;
            }
            if let Some(source) = current.source() {
                current = source;
            } else {
                break;
            }
        }
        false
    };

    if !is_nonce_too_low(&first_err) {
        let decoded = decode_contract_revert(&first_err);
        signer_pool.release(signer_address).await?;
        return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
    }

    // Alloy's recommended `NonceFiller` uses `SimpleNonceManager`: it caches a
    // per-address nonce, increments it atomically before send, and clears/re-syncs
    // the cache when `prepare` observes an RPC error. Unlike ethers'
    // `NonceManagerMiddleware`, there is no public counter setter and no internal
    // send retry after a broadcast failure, so we preserve explicit recovery here:
    // fetch the pending nonce and pin it on the retry call.
    let fresh_nonce = match client
        .get_transaction_count(signer_address)
        .block_id(alloy::eips::BlockId::Number(BlockNumberOrTag::Pending))
        .await
    {
        Ok(nonce) => nonce,
        Err(nonce_err) => {
            tracing::warn!("nonce resync failed: {nonce_err}");
            signer_pool.release(signer_address).await?;
            return Err(anyhow::anyhow!(
                "Failed to send transaction (nonce resync failed): original error: {first_err}, nonce fetch error: {nonce_err}"
            ));
        }
    };

    let retry_call = function_call.nonce(fresh_nonce);
    let tx = match retry_call.send().await {
        Ok(tx) => tx,
        Err(retry_err) => {
            let decoded = decode_contract_revert(&retry_err);
            signer_pool.release(signer_address).await?;
            return Err(anyhow::anyhow!("Failed to send transaction: {decoded}"));
        }
    };

    let result = match tx.get_receipt().await {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    };

    signer_pool.release(signer_address).await?;
    result
}
