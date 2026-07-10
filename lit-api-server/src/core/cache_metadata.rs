//! Secondary metadata index for the shared Lit Action code cache (CPL-351).
//!
//! The primary cache (`ipfs_cache` in `main.rs`) holds the sandboxed action
//! binaries/code keyed by IPFS id. That cache intentionally knows nothing about
//! *who* the code belongs to. This index sits alongside it and provides the
//! "secondary lookup" the ticket asks for: a correlation from a **master user
//! account** (identified by its on-chain account wallet address — the value
//! both master and usage keys resolve to) to the metadata of the cache entries
//! that account has executed.
//!
//! Only descriptive metadata is stored here — size, timestamps, run count.
//! The cached code itself never enters this index and is never exposed by the
//! metadata endpoint.
//!
//! Consistency with the primary cache is maintained by wiring
//! [`CacheMetadataIndex::remove_entry`] into the primary cache's moka
//! `eviction_listener`, so evicted binaries drop their metadata too.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::SystemTime;

/// Descriptive metadata about a single cached action-code entry.
///
/// Deliberately excludes the cached code/binary — this struct is safe to
/// surface over the API.
#[derive(Clone, Debug)]
struct CacheEntryMetadata {
    /// IPFS id (primary cache key) of the cached action code.
    ipfs_id: String,
    /// Size of the cached code in bytes.
    size_bytes: u64,
    /// When this entry was first recorded in the cache.
    created_at: SystemTime,
    /// When this entry was most recently executed.
    last_run_at: SystemTime,
    /// Total number of executions recorded against this entry (across every
    /// account that has run it).
    run_count: u64,
    /// Account wallet addresses (master-account identities) that have executed
    /// this entry. Used both for the reverse lookup and to keep the secondary
    /// index consistent on eviction.
    account_addresses: HashSet<String>,
}

/// A cheap, read-only projection of a cache entry's metadata for a single
/// account lookup. Excludes `account_addresses` so a widely-shared entry does
/// not force a clone of a large `HashSet` on every `GET /cache_metadata`.
#[derive(Clone, Debug)]
pub struct CacheEntrySnapshot {
    pub ipfs_id: String,
    pub size_bytes: u64,
    pub created_at: SystemTime,
    pub last_run_at: SystemTime,
    pub run_count: u64,
    /// Number of distinct accounts correlated with this entry.
    pub account_count: usize,
}

#[derive(Default)]
struct State {
    /// Primary metadata map: IPFS id -> metadata.
    entries: HashMap<String, CacheEntryMetadata>,
    /// Secondary lookup: account wallet address -> IPFS ids that account ran.
    by_account: HashMap<String, HashSet<String>>,
}

/// Thread-safe metadata index correlating cached action code with the master
/// account that executed it. Registered as Rocket managed state.
#[derive(Default)]
pub struct CacheMetadataIndex {
    state: RwLock<State>,
}

impl CacheMetadataIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `account_address` executed the cached entry `ipfs_id`.
    ///
    /// Creates the entry on first sight (stamping `created_at`), and on every
    /// call refreshes `last_run_at`/`size_bytes`, increments `run_count`, and
    /// correlates the entry with the account in the secondary index.
    pub fn record_execution(
        &self,
        ipfs_id: &str,
        size_bytes: u64,
        account_address: &str,
        now: SystemTime,
    ) {
        // Recover from a poisoned lock rather than panic: this runs on the hot
        // action-execution path, and a partially-written best-effort metadata
        // entry is never a correctness or safety hazard.
        let mut st = self.state.write().unwrap_or_else(|e| e.into_inner());

        let entry = st
            .entries
            .entry(ipfs_id.to_string())
            .or_insert_with(|| CacheEntryMetadata {
                ipfs_id: ipfs_id.to_string(),
                size_bytes,
                created_at: now,
                last_run_at: now,
                run_count: 0,
                account_addresses: HashSet::new(),
            });
        entry.size_bytes = size_bytes;
        entry.last_run_at = now;
        entry.run_count = entry.run_count.saturating_add(1);
        entry.account_addresses.insert(account_address.to_string());

        st.by_account
            .entry(account_address.to_string())
            .or_default()
            .insert(ipfs_id.to_string());
    }

    /// Drop all metadata for `ipfs_id`, keeping the secondary index consistent.
    ///
    /// Called from the primary cache's eviction listener when a binary is
    /// evicted (capacity pressure), replaced, or explicitly removed.
    pub fn remove_entry(&self, ipfs_id: &str) {
        // Runs inside the moka eviction listener, which must be panic-free —
        // recover a poisoned lock instead of aborting the eviction.
        let mut st = self.state.write().unwrap_or_else(|e| e.into_inner());
        let Some(meta) = st.entries.remove(ipfs_id) else {
            return;
        };
        for addr in meta.account_addresses {
            let now_empty = if let Some(set) = st.by_account.get_mut(&addr) {
                set.remove(ipfs_id);
                set.is_empty()
            } else {
                false
            };
            if now_empty {
                st.by_account.remove(&addr);
            }
        }
    }

    /// Secondary lookup: metadata for every cache entry `account_address` has
    /// executed. Returns an empty vec for an unknown account.
    pub fn entries_for_account(&self, account_address: &str) -> Vec<CacheEntrySnapshot> {
        // Recover a poisoned lock rather than panic a GET request — a metadata
        // read must never be an availability risk.
        let st = self.state.read().unwrap_or_else(|e| e.into_inner());
        let Some(ids) = st.by_account.get(account_address) else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|id| st.entries.get(id))
            .map(|m| CacheEntrySnapshot {
                ipfs_id: m.ipfs_id.clone(),
                size_bytes: m.size_bytes,
                created_at: m.created_at,
                last_run_at: m.last_run_at,
                run_count: m.run_count,
                account_count: m.account_addresses.len(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const A: &str = "0xaaaa000000000000000000000000000000000000";
    const B: &str = "0xbbbb000000000000000000000000000000000000";

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn records_and_looks_up_by_account() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmA", 100, A, t(1));

        let entries = idx.entries_for_account(A);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ipfs_id, "QmA");
        assert_eq!(entries[0].size_bytes, 100);
        assert_eq!(entries[0].run_count, 1);
        assert_eq!(entries[0].created_at, t(1));
        assert_eq!(entries[0].last_run_at, t(1));
    }

    #[test]
    fn unknown_account_is_empty() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmA", 100, A, t(1));
        assert!(idx.entries_for_account(B).is_empty());
    }

    #[test]
    fn repeated_runs_bump_count_and_last_run_but_keep_created_at() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmA", 100, A, t(1));
        idx.record_execution("QmA", 120, A, t(5));

        let entries = idx.entries_for_account(A);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].run_count, 2);
        assert_eq!(entries[0].created_at, t(1));
        assert_eq!(entries[0].last_run_at, t(5));
        // size reflects the latest recorded value.
        assert_eq!(entries[0].size_bytes, 120);
    }

    #[test]
    fn shared_entry_correlates_to_multiple_accounts() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmShared", 50, A, t(1));
        idx.record_execution("QmShared", 50, B, t(2));

        assert_eq!(idx.entries_for_account(A).len(), 1);
        assert_eq!(idx.entries_for_account(B).len(), 1);
        // Both accounts point at the same underlying entry.
        let a = &idx.entries_for_account(A)[0];
        assert_eq!(a.run_count, 2);
        assert_eq!(a.account_count, 2);
    }

    #[test]
    fn eviction_removes_entry_and_cleans_secondary_index() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmA", 100, A, t(1));
        idx.record_execution("QmB", 100, A, t(1));

        idx.remove_entry("QmA");

        let entries = idx.entries_for_account(A);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ipfs_id, "QmB");
    }

    #[test]
    fn eviction_of_last_entry_drops_account_bucket() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmShared", 50, A, t(1));
        idx.record_execution("QmShared", 50, B, t(1));

        idx.remove_entry("QmShared");

        // Both accounts' buckets must be gone, not just the entry.
        assert!(idx.entries_for_account(A).is_empty());
        assert!(idx.entries_for_account(B).is_empty());
    }

    #[test]
    fn removing_unknown_entry_is_a_noop() {
        let idx = CacheMetadataIndex::new();
        idx.record_execution("QmA", 100, A, t(1));
        idx.remove_entry("QmDoesNotExist");
        assert_eq!(idx.entries_for_account(A).len(), 1);
    }
}
