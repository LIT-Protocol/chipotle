use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ethers::middleware::{NonceManagerMiddleware, SignerMiddleware};
use ethers::providers::{Http, Provider};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::H160;

use crate::accounts::signable_contract::SigningClient;
use crate::config::GLOBAL_NODE_CONFIG;
use crate::dstack::v1::get_lit_payer_key;

const STALE_LEASE_SECS: u64 = 10;
const CLEANUP_INTERVAL_SECS: u64 = 10;

pub struct SigningPoolEntry {
    client: Arc<SigningClient>,
    address: H160,
    in_use: bool,
    in_use_since: Option<Instant>,
}

/// Returned by `SignerPool::request`. Contains the signing client and its
/// address. Pass `address` back to `SignerPool::release` when done.
pub struct SignerHandle {
    pub client: Arc<SigningClient>,
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
pub async fn start_signer_pool(pool_size: usize) -> Result<SignerPool> {
    let (tx, rx) = flume::unbounded::<SigningPoolMessage>();

    let entries = get_signer_entries(pool_size).await?;
    tokio::spawn(run_pool(entries, rx));

    Ok(SignerPool { tx })
}

/// Get the signer entries for the signer pool.
pub async fn get_signer_entries(pool_size: usize) -> Result<Vec<SigningPoolEntry>> {
    let node_config = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = node_config.chain.info();

    let mut entries: Vec<SigningPoolEntry> = Vec::with_capacity(pool_size);
    for i in 1..=(pool_size as u16) {
        let secret = get_lit_payer_key(i)
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
        });
    }

    Ok(entries)
}

async fn run_pool(mut entries: Vec<SigningPoolEntry>, rx: flume::Receiver<SigningPoolMessage>) {
    let stale = Duration::from_secs(STALE_LEASE_SECS);
    let mut signer_count = entries.len();
    let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
    interval.tick().await; // discard the immediate first tick

    loop {
        tokio::select! {
            msg = rx.recv_async() => {
                match msg {
                    Ok(SigningPoolMessage::Request { reply }) => {
                        if let Some(entry) = entries.iter_mut().find(|e| !e.in_use) {
                            entry.in_use = true;
                            entry.in_use_since = Some(Instant::now());
                            let _ = reply.send(SignerHandle {
                                client: Arc::clone(&entry.client),
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
                                    client: Arc::clone(&entry.client),
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
                let new_signer_count = match crate::accounts::get_signer_count().await {
                    Ok(count) => count,
                    Err(e) => {
                        tracing::error!("signer_pool: failed to get signer count: {e}");
                        signer_count
                    }
                };
                if new_signer_count > signer_count {
                    match  get_signer_entries(new_signer_count).await {
                        Ok(new_entries) => {
                            entries = new_entries;
                            signer_count = new_signer_count;
                        }
                        Err(e) => {
                            tracing::error!("signer_pool: failed to get signer entries: {e}");
                        }
                    };
                }
                let now = Instant::now();
                for entry in entries.iter_mut() {
                    if let (true, Some(since)) = (entry.in_use, entry.in_use_since)
                        && now.duration_since(since) > stale {
                            tracing::warn!(
                                "signer_pool: freeing stale lease for {:?} (held {:?})",
                                entry.address,
                                now.duration_since(since),
                            );
                            entry.in_use = false;
                            entry.in_use_since = None;

                    }
                }
            }
        }
    }
}
