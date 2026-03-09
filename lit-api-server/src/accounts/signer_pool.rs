use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ethers::middleware::{NonceManagerMiddleware, SignerMiddleware};
use ethers::providers::{Http, Provider};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::{H160, U256};
use ethers_providers::Middleware;

use crate::accounts::signable_contract::{
    SigningClient, get_admin_api_payer_contract, get_admin_api_signer,
    get_read_only_account_config_contract,
};
use crate::accounts::{get_api_payer_count, get_rebalance_amount};
use crate::config::GLOBAL_NODE_CONFIG;
use crate::dstack::v1::get_lit_payer_key;

const STALE_LEASE_SECS: u64 = 10;
const CLEANUP_INTERVAL_SECS: u64 = 5;

#[derive(Clone)]
pub struct SigningPoolEntry {
    client: Arc<SigningClient>,
    address: H160,
    in_use: bool,
    in_use_since: Option<Instant>,
    last_request: Instant,
}

/// Returned by `SignerPool::request`. Contains the signing client and its
/// address. Pass `address` back to `SignerPool::release` when done.
pub struct SignerHandle {
    pub client: Option<Arc<SigningClient>>,
    pub address: H160,
}

pub enum SigningPoolMessage {
    Request { reply: flume::Sender<SignerHandle> },
    Release { address: H160 },
}

/// A clone-cheap handle to the background signer pool task.
#[derive(Clone)]
pub struct SignerPool {
    tx: flume::Sender<SigningPoolMessage>,
}

impl SignerPool {
    /// Borrow a signing client from the pool. Returns the client and its
    /// address. Call `release(address)` when the transaction is complete.
    pub async fn request(&self) -> Result<SignerHandle> {
        let (reply_tx, reply_rx) = flume::bounded(1);
        self.tx
            .send_async(SigningPoolMessage::Request { reply: reply_tx })
            .await
            .map_err(|e| anyhow::anyhow!("signer pool request send: {e}"))?;
        reply_rx
            .recv_async()
            .await
            .map_err(|e| anyhow::anyhow!("signer pool request recv: {e}"))
    }

    /// Return a previously borrowed signer back to the pool.
    pub async fn release(&self, address: H160) -> Result<()> {
        self.tx
            .send_async(SigningPoolMessage::Release { address })
            .await
            .map_err(|e| anyhow::anyhow!("signer pool release send: {e}"))
    }
}

/// Spawn the signer pool background task and return a `SignerPool` handle.
///
/// Creates `pool_size` signing clients using key indices 1..=pool_size via
/// `get_lit_payer_key`. Must be called after `init_config()`.
pub async fn start_signer_pool() -> Result<SignerPool> {
    let (tx, rx) = flume::unbounded::<SigningPoolMessage>();

    let pool_size = get_api_payer_count().await?;
    tracing::info!("signer_pool: attempting to start with pool size: {pool_size}");

    let entries = get_signer_entries(1, pool_size).await?;

    tokio::spawn(run_pool(entries, rx));

    Ok(SignerPool { tx })
}

/// Get the signer entries for the signer pool.
pub async fn get_signer_entries(
    start_index: usize,
    pool_size: usize,
) -> Result<Vec<SigningPoolEntry>> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();

    if pool_size == 0 {
        return Ok(Vec::new());
    }

    let mut entries: Vec<SigningPoolEntry> = Vec::with_capacity(pool_size);
    for i in start_index..=(start_index + pool_size - 1) {
        let secret = get_lit_payer_key(i as u16)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);
        let address = wallet.address();
        let provider =
            Provider::<Http>::try_from(chain_info.rpc_url)?.interval(Duration::from_secs(2));
        let signer = SignerMiddleware::new(provider, wallet);
        let nonce_manager = NonceManagerMiddleware::new(signer, address);
        tracing::info!("signer_pool: created signer {} address={:?}", i, address);
        entries.push(SigningPoolEntry {
            client: Arc::new(nonce_manager),
            address,
            in_use: false,
            in_use_since: None,
            last_request: Instant::now(),
        });
    }

    Ok(entries)
}

async fn run_pool(mut entries: Vec<SigningPoolEntry>, rx: flume::Receiver<SigningPoolMessage>) {
    let mut payer_count = entries.len();
    tracing::info!("signer_pool: signer count: {payer_count}");
    let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
    interval.tick().await; // discard the immediate first tick

    loop {
        tokio::select! {
            msg = rx.recv_async() => {
                match msg {
                    Ok(SigningPoolMessage::Request { reply }) => {

                        if entries.is_empty() {
                            tracing::warn!("signer_pool: no signers available, returning None");
                            let _ = reply.send(SignerHandle {
                                client: None,
                                address: H160::zero(),
                            });
                            continue;
                        }

                        entries.sort_by_key(|k| k.last_request);
                        if let Some(entry) = entries.iter_mut().find(|e| !e.in_use) {
                            entry.in_use = true;
                            entry.in_use_since = Some(Instant::now());
                            tracing::info!("signer_pool: granted lease to {:?}", entry.address);
                            let _ = reply.send(SignerHandle {
                                client: Some(Arc::clone(&entry.client)),
                                address: entry.address,
                            });
                        } else {
                            // All signers busy — return the least-recently borrowed one
                            // as a fallback so the caller is never blocked indefinitely.
                            tracing::warn!("signer_pool: all signers in use, returning oldest lease as fallback");
                            if let Some(entry) = entries
                                .iter_mut()
                                .min_by_key(|e| e.in_use_since.unwrap_or(Instant::now()))
                            {
                                let _ = reply.send(SignerHandle {
                                    client: Some(Arc::clone(&entry.client)),
                                    address: entry.address,
                                });
                            }
                        }
                    }
                    Ok(SigningPoolMessage::Release { address }) => {
                        if let Some(entry) =
                            entries.iter_mut().find(|e| e.address == address)
                        {
                            entry.in_use = false;
                            entry.last_request = Instant::now();
                            entry.in_use_since = None;
                        } else {
                            tracing::warn!(
                                "signer_pool: release for unknown address {:?}",
                                address
                            );
                        }
                    }
                    Err(_) => {
                        tracing::info!("signer_pool: channel closed, shutting down");
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                check_for_new_api_payer_count(&mut entries, &mut payer_count).await;
                release_stale_leases(&mut entries).await;
            }
        }
    }
}

async fn check_for_new_api_payer_count(
    entries: &mut Vec<SigningPoolEntry>,
    payer_count: &mut usize,
) {
    let new_api_payer_count = match crate::accounts::get_requested_api_payer_count().await {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("signer_pool: failed to get signer count: {e}");
            *payer_count
        }
    };

    if new_api_payer_count == *payer_count {
        return;
    }

    let old_entries = entries.clone();

    if new_api_payer_count > *payer_count {
        match get_signer_entries(*payer_count + 1, new_api_payer_count - *payer_count).await {
            Ok(new_entries) => {
                entries.extend(new_entries);
                *payer_count = new_api_payer_count;
            }
            Err(e) => {
                tracing::error!("signer_pool: failed to get signer entries: {e}");
            }
        };
    } else if new_api_payer_count < *payer_count {
        entries.truncate(new_api_payer_count);
        *payer_count = new_api_payer_count;
    };

    if let Err(e) = set_api_payers(entries.clone()).await {
        tracing::error!("signer_pool: failed to set api payers: {e}");
        return;
    }

    if let Ok(rebalance_amount) = get_rebalance_amount().await
        && rebalance_amount > U256::zero()
        && let Err(e) =
            rebalance_entries(rebalance_amount, old_entries.clone(), entries.clone()).await
    {
        tracing::error!("signer_pool: failed to rebalance entries: {e}");
    }
}

async fn set_api_payers(entries: Vec<SigningPoolEntry>) -> Result<()> {
    let contract = get_admin_api_payer_contract().await?;
    let api_payers = entries.iter().map(|e| e.address).collect();

    tracing::info!("signer_pool: setting api payers: {:?}", api_payers);

    let function_call = contract.set_api_payers(api_payers);
    let tx = function_call.send().await;

    if let Err(e) = tx {
        return Err(anyhow::anyhow!("Failed to set api payers: {e}"));
    }

    tracing::info!("signer_pool: api payers set successfully");
    Ok(())
}

async fn release_stale_leases(entries: &mut [SigningPoolEntry]) {
    let stale = Duration::from_secs(STALE_LEASE_SECS);
    let now = Instant::now();
    for entry in entries.iter_mut() {
        if let (true, Some(since)) = (entry.in_use, entry.in_use_since)
            && now.duration_since(since) > stale
        {
            tracing::warn!(
                "signer_pool: freeing stale lease for {:?} (held {:?})",
                entry.address,
                now.duration_since(since)
            );
            entry.in_use = false;
            entry.in_use_since = None;
        }
    }
}

async fn rebalance_entries(
    rebalance_amount: U256,
    old_entries: Vec<SigningPoolEntry>,
    new_entries: Vec<SigningPoolEntry>,
) -> Result<()> {
    let admin_signer = get_admin_api_signer().await?;
    let read_only_client = get_read_only_account_config_contract().await?;
    let admin_wallet = read_only_client.admin_api_payer_account().call().await?;

    let chain_info = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = chain_info.chain.info();

    let block_number = None;
    let gas_required = admin_signer.get_gas_price().await? * 21000 * 2;
    tracing::info!("signer_pool: gas price: {gas_required}");

    for entry in old_entries.iter() {
        let current_funds = admin_signer.get_balance(entry.address, None).await?;
        if current_funds < gas_required {
            tracing::error!(
                "signer_pool: not enough funds to rebalance:   {:?} has {current_funds} < {gas_required}",
                entry.address
            );
            continue;
        }
        let req = ethers::types::Eip1559TransactionRequest::new()
            .to(admin_wallet)
            .value(current_funds - gas_required)
            .chain_id(chain_info.chain_id);

        let tx = entry.client.send_transaction(req, block_number).await;
        match tx {
            Ok(tx) => {
                tx.await?;
                tracing::info!(
                    "signer_pool: repatriated funds to admin wallet from {:?}",
                    entry.address
                );
            }
            Err(e) => {
                tracing::error!(
                    "signer_pool: failed to repatriate funds to admin wallet from {:?}: {e}",
                    entry.address
                );
            }
        }
    }

    for entry in new_entries.iter() {
        let req = ethers::types::Eip1559TransactionRequest::new()
            .to(entry.address)
            .value(rebalance_amount)
            .chain_id(chain_info.chain_id);

        let tx = admin_signer.send_transaction(req, block_number).await;
        match tx {
            Ok(tx) => {
                tx.await?;
                tracing::info!("signer_pool: funded {:?}", entry.address);
            }
            Err(e) => {
                tracing::error!("signer_pool: failed to fund {:?}: {e}", entry.address);
            }
        }
    }

    Ok(())
}
