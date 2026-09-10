use std::time::{Duration, Instant};

use alloy::primitives::{Address, B256, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use anyhow::Result;

use crate::accounts::decode_revert::decode_contract_revert;
use crate::accounts::signable_contract::{
    RPC_STEP_TIMEOUT, SigningClient, fetch_pending_nonce, get_account_config_contract,
    get_admin_api_signer_with_address, get_read_only_account_config_contract, invalidate_nonce,
    reserve_nonce, wait_for_receipt,
};
use crate::accounts::{get_api_payer_count, get_rebalance_amount};
use crate::config::GLOBAL_NODE_CONFIG;
use crate::dstack::v1::get_lit_payer_key;

/// How long a lease may be held before the cleanup pass force-frees it.
///
/// Must exceed the worst-case bounded `send_transaction`: simulation (10s) +
/// nonce fetch (10s) + broadcast (10s) + receipt (30s + 10s grace), plus one
/// nonce-collision retry (10s + 10s + 40s) ≈ 130s. Below that, cleanup frees
/// leases whose borrowers are legitimately still working, forcing lease
/// sharing on every slow transaction. With lease ids and reserved nonces a
/// premature free is no longer dangerous, only wasteful — this is purely a
/// leak recovery net for borrowers that died without releasing (panic,
/// cancelled request future).
const STALE_LEASE_SECS: u64 = 150;
const CLEANUP_INTERVAL_SECS: u64 = 5;

/// Hard bound on the periodic housekeeping pass (payer-count read, signer
/// creation, setApiPayers). It runs inside the pool dispatcher's select loop,
/// so without a bound one blackholed RPC would stall every lease grant and
/// release. Rebalancing is spawned off-loop and not covered by this.
const HOUSEKEEPING_TIMEOUT: Duration = Duration::from_secs(30);

fn record_idle_signers(entries: &[SigningPoolEntry]) {
    let idle = entries.iter().filter(|e| !e.in_use).count();
    metrics::gauge!("signer_pool.idle_signers").set(idle as f64);
}

#[derive(Clone)]
pub struct SigningPoolEntry {
    client: SigningClient,
    address: Address,
    in_use: bool,
    in_use_since: Option<Instant>,
    last_request: Instant,
    /// Id of the most recent lease granted on this entry. A release only
    /// takes effect when its id matches, so a slow borrower whose lease was
    /// force-freed (and possibly re-granted) can't free the next borrower's
    /// lease out from under them.
    lease_id: u64,
}

/// A borrowed signer's identity: its address plus the id of the specific
/// lease grant. Pass the whole lease back to `SignerPool::release`.
#[derive(Clone, Copy, Debug)]
pub struct SignerLease {
    pub address: Address,
    pub id: u64,
}

/// Returned by `SignerPool::request`. Contains the signing client and the
/// lease identifying this borrow.
pub struct SignerHandle {
    pub client: Option<SigningClient>,
    pub lease: SignerLease,
}

pub enum SigningPoolMessage {
    Request { reply: flume::Sender<SignerHandle> },
    Release { lease: SignerLease },
}

/// A clone-cheap handle to the background signer pool task.
#[derive(Clone)]
pub struct SignerPool {
    tx: flume::Sender<SigningPoolMessage>,
}

impl SignerPool {
    /// Borrow a signing client from the pool. Returns the client and the
    /// lease identifying the borrow. Call `release(&lease)` when the
    /// transaction is complete.
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

    /// Return a previously borrowed signer back to the pool. A no-op if the
    /// lease was already force-freed and re-granted to someone else.
    pub async fn release(&self, lease: &SignerLease) -> Result<()> {
        self.tx
            .send_async(SigningPoolMessage::Release { lease: *lease })
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
        let wallet = PrivateKeySigner::from_bytes(&B256::from_slice(&secret))?
            .with_chain_id(Some(chain_info.chain_id));
        let address = wallet.address();
        let client = crate::accounts::signable_contract::signer_provider(wallet)?;
        tracing::info!("signer_pool: created signer {} address={:?}", i, address);
        entries.push(SigningPoolEntry {
            client,
            address,
            in_use: false,
            in_use_since: None,
            last_request: Instant::now(),
            lease_id: 0,
        });
    }

    Ok(entries)
}

/// Grant a lease: the idle signer with the oldest `last_request`, or — when
/// every signer is busy — a shared lease on the longest-held busy one so the
/// caller is never blocked indefinitely. A shared grant takes over the
/// entry's lease id (the newest borrower owns it; the previous borrower's
/// release becomes a no-op) and refreshes `in_use_since` so consecutive
/// fallback grants rotate across busy signers instead of piling onto one.
fn grant_lease(
    entries: &mut [SigningPoolEntry],
    next_lease_id: &mut u64,
    now: Instant,
) -> SignerHandle {
    if entries.is_empty() {
        tracing::warn!("signer_pool: no signers available, returning None");
        return SignerHandle {
            client: None,
            lease: SignerLease {
                address: Address::ZERO,
                id: 0,
            },
        };
    }

    *next_lease_id += 1;
    let id = *next_lease_id;

    entries.sort_by_key(|k| k.last_request);
    let entry = match entries.iter_mut().find(|e| !e.in_use) {
        Some(idle) => {
            idle.in_use = true;
            tracing::info!("signer_pool: granted lease to {:?}", idle.address);
            idle
        }
        None => {
            tracing::warn!("signer_pool: all signers in use, sharing oldest lease as fallback");
            entries
                .iter_mut()
                .min_by_key(|e| e.in_use_since.unwrap_or(now))
                .expect("entries is non-empty")
        }
    };
    entry.in_use_since = Some(now);
    entry.lease_id = id;
    SignerHandle {
        client: Some(entry.client.clone()),
        lease: SignerLease {
            address: entry.address,
            id,
        },
    }
}

/// Free a lease, but only if it is still the entry's current lease: a borrower
/// whose lease was force-freed as stale (and possibly re-granted) must not
/// free the newer borrower's lease.
fn release_lease(entries: &mut [SigningPoolEntry], lease: SignerLease, now: Instant) {
    match entries.iter_mut().find(|e| e.address == lease.address) {
        Some(entry) if entry.in_use && entry.lease_id == lease.id => {
            entry.in_use = false;
            entry.in_use_since = None;
            entry.last_request = now;
        }
        Some(_) => {
            tracing::debug!(
                "signer_pool: ignoring release of superseded lease {} for {:?}",
                lease.id,
                lease.address
            );
        }
        None => {
            tracing::warn!(
                "signer_pool: release for unknown address {:?}",
                lease.address
            );
        }
    }
}

async fn run_pool(mut entries: Vec<SigningPoolEntry>, rx: flume::Receiver<SigningPoolMessage>) {
    let mut payer_count = entries.len();
    let mut next_lease_id: u64 = 0;
    tracing::info!("signer_pool: signer count: {payer_count}");
    record_idle_signers(&entries);
    let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
    interval.tick().await; // discard the immediate first tick

    loop {
        tokio::select! {
            msg = rx.recv_async() => {
                match msg {
                    Ok(SigningPoolMessage::Request { reply }) => {
                        let handle = grant_lease(&mut entries, &mut next_lease_id, Instant::now());
                        let _ = reply.send(handle);
                        record_idle_signers(&entries);
                    }
                    Ok(SigningPoolMessage::Release { lease }) => {
                        release_lease(&mut entries, lease, Instant::now());
                        record_idle_signers(&entries);
                    }
                    Err(_) => {
                        tracing::info!("signer_pool: channel closed, shutting down");
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                // Bounded: this runs on the dispatcher task, so an unbounded
                // RPC stall here would freeze every lease grant and release.
                if tokio::time::timeout(
                    HOUSEKEEPING_TIMEOUT,
                    check_for_new_api_payer_count(&mut entries, &mut payer_count),
                )
                .await
                .is_err()
                {
                    tracing::error!(
                        "signer_pool: housekeeping timed out after {HOUSEKEEPING_TIMEOUT:?}"
                    );
                }
                release_stale_leases(&mut entries, Instant::now());
                record_idle_signers(&entries);
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
        && rebalance_amount > alloy::primitives::U256::ZERO
    {
        // Rebalancing waits on funding-transaction receipts (up to ~40s
        // each). Run it off the dispatcher task so lease grants never queue
        // behind it, and outside HOUSEKEEPING_TIMEOUT so a long but healthy
        // rebalance isn't clipped mid-way.
        let old_entries = old_entries.clone();
        let new_entries = entries.clone();
        tokio::spawn(async move {
            if let Err(e) = rebalance_entries(rebalance_amount, old_entries, new_entries).await {
                tracing::error!("signer_pool: failed to rebalance entries: {e}");
            }
        });
    }
}

async fn set_api_payers(entries: Vec<SigningPoolEntry>) -> Result<()> {
    let (admin_signer, admin_address) = get_admin_api_signer_with_address().await?;
    let contract = get_account_config_contract(admin_signer.clone()).await?;
    let api_payers = entries.iter().map(|e| e.address).collect();

    tracing::info!("signer_pool: setting api payers: {:?}", api_payers);

    let function_call = contract.setApiPayers(api_payers);

    // Call-before-send: dry-run to surface a decoded revert reason before
    // broadcasting the admin transaction.
    if let Err(sim_err) = function_call.call().await {
        let decoded = decode_contract_revert(&sim_err);
        return Err(anyhow::anyhow!(
            "Failed to set api payers (simulation): {decoded}"
        ));
    }

    // Admin sends are rare and serialized (one housekeeping pass at a time),
    // so pin a freshly fetched pending nonce per send rather than involving
    // any nonce cache — pending accounts for the spawned rebalancer's
    // possibly in-flight admin funding transactions.
    let nonce = fetch_pending_nonce(&admin_signer, admin_address).await?;

    let tx = function_call.nonce(nonce).send().await;

    if let Err(e) = tx {
        return Err(anyhow::anyhow!("Failed to set api payers: {e}"));
    }

    tracing::info!("signer_pool: api payers set successfully");
    Ok(())
}

fn release_stale_leases(entries: &mut [SigningPoolEntry], now: Instant) {
    let stale = Duration::from_secs(STALE_LEASE_SECS);
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
            // Rotate a force-freed signer to the back of the LRU order.
            // last_request is otherwise only updated on a clean release, so a
            // signer whose borrower hangs keeps the oldest last_request and is
            // re-granted first — during the 2026-09-03 incident two wedged
            // payers absorbed nearly all lease grants this way, hanging every
            // write endpoint they were handed to.
            entry.last_request = now;
        }
    }
}

async fn rebalance_entries(
    rebalance_amount: U256,
    old_entries: Vec<SigningPoolEntry>,
    new_entries: Vec<SigningPoolEntry>,
) -> Result<()> {
    let (admin_signer, admin_address) = get_admin_api_signer_with_address().await?;
    let read_only_client = get_read_only_account_config_contract().await?;
    let admin_wallet = read_only_client.adminApiPayerAccount().call().await?;

    let chain_info = GLOBAL_NODE_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("Node configuration not found"))?;
    let chain_info = chain_info.chain.info();

    let gas_required = U256::from(admin_signer.get_gas_price().await?) * U256::from(21000 * 2);
    tracing::info!("signer_pool: gas price: {gas_required}");

    for entry in old_entries.iter() {
        let current_funds = admin_signer.get_balance(entry.address).await?;
        if current_funds < gas_required {
            tracing::error!(
                "signer_pool: not enough funds to rebalance:   {:?} has {current_funds} < {gas_required}",
                entry.address
            );
            continue;
        }
        // Payer nonces come from the same allocator contract writes use, so a
        // repatriation can't collide with (or be gapped by) alloy's separate
        // NonceFiller cache.
        let nonce = match reserve_nonce(&entry.client, entry.address).await {
            Ok(nonce) => nonce,
            Err(e) => {
                tracing::error!(
                    "signer_pool: skipping repatriation from {:?}: {e}",
                    entry.address
                );
                continue;
            }
        };
        let req = TransactionRequest {
            to: Some(alloy::primitives::TxKind::Call(admin_wallet)),
            value: Some(current_funds - gas_required),
            chain_id: Some(chain_info.chain_id),
            nonce: Some(nonce),
            ..Default::default()
        };

        // One failed wallet must not abort the rest of the loop — the
        // remaining wallets would otherwise never be drained (and this
        // function is not re-run until the payer count changes again).
        match tokio::time::timeout(RPC_STEP_TIMEOUT, entry.client.send_transaction(req)).await {
            Ok(Ok(tx)) => match wait_for_receipt(tx, entry.address).await {
                Ok(_) => tracing::info!(
                    "signer_pool: repatriated funds to admin wallet from {:?}",
                    entry.address
                ),
                Err(e) => tracing::error!(
                    "signer_pool: repatriation from {:?} not confirmed: {e}",
                    entry.address
                ),
            },
            Ok(Err(e)) => {
                invalidate_nonce(entry.address);
                tracing::error!(
                    "signer_pool: failed to repatriate funds to admin wallet from {:?}: {e}",
                    entry.address
                );
            }
            Err(_) => {
                invalidate_nonce(entry.address);
                tracing::error!(
                    "signer_pool: repatriation broadcast from {:?} timed out after {RPC_STEP_TIMEOUT:?}",
                    entry.address
                );
            }
        }
    }

    for entry in new_entries.iter() {
        // Admin sends are serialized within this loop and rare elsewhere; a
        // fresh pending fetch per send is the whole allocation policy (pending
        // counts the previous funding tx even if its receipt timed out).
        let nonce = match fetch_pending_nonce(&admin_signer, admin_address).await {
            Ok(nonce) => nonce,
            Err(e) => {
                tracing::error!("signer_pool: skipping funding of {:?}: {e}", entry.address);
                continue;
            }
        };
        let req = TransactionRequest {
            to: Some(alloy::primitives::TxKind::Call(entry.address)),
            value: Some(rebalance_amount),
            chain_id: Some(chain_info.chain_id),
            nonce: Some(nonce),
            ..Default::default()
        };

        match tokio::time::timeout(RPC_STEP_TIMEOUT, admin_signer.send_transaction(req)).await {
            Ok(Ok(tx)) => match wait_for_receipt(tx, admin_address).await {
                Ok(_) => tracing::info!("signer_pool: funded {:?}", entry.address),
                Err(e) => tracing::error!(
                    "signer_pool: funding of {:?} not confirmed: {e}",
                    entry.address
                ),
            },
            Ok(Err(e)) => {
                tracing::error!("signer_pool: failed to fund {:?}: {e}", entry.address);
            }
            Err(_) => {
                tracing::error!(
                    "signer_pool: funding broadcast for {:?} timed out after {RPC_STEP_TIMEOUT:?}",
                    entry.address
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::providers::ProviderBuilder;

    fn entry(
        address_byte: u8,
        in_use: bool,
        in_use_since: Option<Instant>,
        last_request: Instant,
    ) -> SigningPoolEntry {
        // The provider is never dialed in these tests; any URL works.
        let client = ProviderBuilder::new()
            .connect_http("http://127.0.0.1:1".parse().expect("static url"))
            .erased();
        SigningPoolEntry {
            client,
            address: Address::repeat_byte(address_byte),
            in_use,
            in_use_since,
            last_request,
            lease_id: 0,
        }
    }

    #[test]
    fn stale_lease_is_freed_and_rotated_to_back_of_lru() {
        let now = Instant::now();
        let long_ago = now
            .checked_sub(Duration::from_secs(STALE_LEASE_SECS * 3))
            .expect("test clock underflow");
        let mut entries = vec![
            // Wedged signer: borrowed long ago, never released.
            entry(0x11, true, Some(long_ago), long_ago),
            // Healthy signer released recently.
            entry(0x22, false, None, now),
        ];

        release_stale_leases(&mut entries, now);

        assert!(!entries[0].in_use, "stale lease must be freed");
        assert!(entries[0].in_use_since.is_none());
        // The freed signer must not keep the oldest last_request, or run_pool
        // (which grants the oldest-last_request idle signer first) would hand
        // it every subsequent request.
        assert!(
            entries[0].last_request >= entries[1].last_request,
            "force-freed signer must rotate behind the healthy one"
        );
    }

    #[test]
    fn fresh_lease_is_left_alone() {
        let now = Instant::now();
        let mut entries = vec![entry(0x11, true, Some(now), now)];

        release_stale_leases(&mut entries, now);

        assert!(entries[0].in_use, "fresh lease must not be freed");
        assert!(entries[0].in_use_since.is_some());
    }

    #[test]
    fn grant_prefers_oldest_idle_and_issues_unique_lease_ids() {
        let now = Instant::now();
        let older = now
            .checked_sub(Duration::from_secs(60))
            .expect("test clock underflow");
        let mut entries = vec![
            entry(0x11, false, None, now),
            entry(0x22, false, None, older),
        ];
        let mut next_id = 0;

        let first = grant_lease(&mut entries, &mut next_id, now);
        let second = grant_lease(&mut entries, &mut next_id, now);

        assert_eq!(
            first.lease.address,
            Address::repeat_byte(0x22),
            "oldest last_request goes first"
        );
        assert_eq!(second.lease.address, Address::repeat_byte(0x11));
        assert_ne!(first.lease.id, second.lease.id, "lease ids must be unique");
    }

    #[test]
    fn stale_release_cannot_free_a_newer_borrowers_lease() {
        let now = Instant::now();
        let mut entries = vec![entry(0x11, false, None, now)];
        let mut next_id = 0;

        // A borrows the only signer; the cleanup pass force-frees the lease
        // (borrower presumed dead); B borrows the same signer.
        let a = grant_lease(&mut entries, &mut next_id, now);
        release_stale_leases(
            &mut entries,
            now + Duration::from_secs(STALE_LEASE_SECS + 1),
        );
        let b = grant_lease(&mut entries, &mut next_id, now);
        assert!(entries[0].in_use, "B holds the signer");

        // A turns out to be alive and releases late: must be a no-op.
        release_lease(&mut entries, a.lease, now);
        assert!(
            entries[0].in_use,
            "A's superseded release must not free B's lease"
        );

        // B's own release works.
        release_lease(&mut entries, b.lease, now);
        assert!(!entries[0].in_use, "B's release frees the signer");
    }

    #[test]
    fn all_busy_fallback_shares_and_rotates_across_busy_signers() {
        let start = Instant::now();
        let mut entries = vec![
            entry(0x11, false, None, start),
            entry(0x22, false, None, start),
        ];
        let mut next_id = 0;

        let a = grant_lease(&mut entries, &mut next_id, start);
        let later = start + Duration::from_secs(1);
        let b = grant_lease(&mut entries, &mut next_id, later);
        assert_ne!(a.lease.address, b.lease.address);

        // Pool exhausted: fallback shares the longest-held lease (A's signer)
        // and refreshes in_use_since, so the next fallback rotates to the
        // other busy signer instead of piling onto the same one.
        let c = grant_lease(&mut entries, &mut next_id, later + Duration::from_secs(1));
        assert_eq!(c.lease.address, a.lease.address, "shares longest-held");
        let d = grant_lease(&mut entries, &mut next_id, later + Duration::from_secs(2));
        assert_eq!(d.lease.address, b.lease.address, "rotates to next busy");

        // The shared grant took over ownership: A's release is now a no-op,
        // C's release frees the signer.
        let a_entry_idx = entries
            .iter()
            .position(|e| e.address == a.lease.address)
            .expect("entry exists");
        release_lease(&mut entries, a.lease, later);
        assert!(
            entries[a_entry_idx].in_use,
            "A's superseded release ignored"
        );
        release_lease(&mut entries, c.lease, later);
        assert!(!entries[a_entry_idx].in_use, "C's release frees the signer");
    }

    #[test]
    fn empty_pool_returns_no_client() {
        let mut entries: Vec<SigningPoolEntry> = Vec::new();
        let mut next_id = 0;

        let handle = grant_lease(&mut entries, &mut next_id, Instant::now());
        assert!(handle.client.is_none());
    }
}
