//! Global blockchain data cache.
//!
//! Caches the results of on-chain permission checks (`canExecuteAction`,
//! `canUseWalletInAction`) and wallet derivation lookups (`getWalletDerivation`)
//! so that repeated calls for the same API key and relevant parameters avoid
//! redundant contract calls.
//!
//! # Design: a dumb verdict memoizer
//!
//! The source of truth for permissions is the chain. This cache is *only* a
//! performance optimization in front of it. It deliberately models nothing
//! about accounts, master/usage keys, groups, or scopes — it memoizes opaque
//! contract verdicts keyed by `(api_key_hash, params)`, exactly as the contract
//! resolves them. A cache miss simply asks the chain; a denial is never special.
//!
//! ## Invalidation: one global generation counter
//!
//! Permission *mutations* are rare administrative operations; permission
//! *checks* are the hot path. Given that ratio, we don't try to map a given
//! on-chain mutation to the specific cache entries it affects (which would
//! require the server to reconstruct the on-chain account graph — every master
//! key, usage key, group and scope). Instead, **any** permission-relevant
//! mutation bumps a single global generation counter embedded in every cache
//! key, so all subsequent lookups miss and re-read from chain. Stale entries
//! from older generations are never read again and age out by TTL.
//!
//! This is strictly *more* invalidation than necessary (a change to one account
//! briefly invalidates verdicts for all accounts), which makes it always safe:
//! it can over-invalidate, never serve stale. The cost is a small burst of
//! chain re-reads right after a rare mutation, coalesced per key by
//! `try_get_with` so the steady-state load is at most one read per
//! (key, params) per generation.
//!
//! The bump is driven by an on-chain event listener ([`crate::account_events`])
//! plus the write-path hooks in [`super`] (which call [`invalidate_all`]
//! directly for instant invalidation of mutations this process performs). The
//! listener also covers mutations this process never sees — ChainSecured
//! wallet-signed transactions sent directly to the contract, or mutations
//! performed by another replica.
//!
//! ## TTL: a backstop, not the freshness mechanism
//!
//! Invalidation is what keeps the cache fresh; the TTL is only insurance for the
//! windows the listener can't cover (missed-block gaps, a replica between boot
//! and its first poll). A single uniform TTL applies to every entry — there is
//! no positive/negative asymmetry, because the cache stores verdicts, not
//! policy, and the listener bounds staleness far tighter than the TTL anyway.
//!
//! ## Stateless / horizontally scalable by construction
//!
//! The generation counter is process-local. Each replica runs its own listener
//! and bumps its own counter off the same chain logs, so replicas converge
//! independently with no shared state. A freshly booted replica starts with an
//! empty cache (everything misses → everything is fresh) and a listener anchored
//! at the current block; it never needs history.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alloy::primitives::{Address, U256};
use moka::future::Cache;

/// Uniform TTL for every cached entry — a backstop for the windows the on-chain
/// listener can't cover (missed-block gaps, a replica between boot and its first
/// poll). Invalidation, not expiry, is the primary freshness mechanism, so this
/// is generous; it is not load-bearing for correctness while the listener runs.
const CACHE_TTL_SECS: u64 = 300;

/// Maximum entries per cache.
const MAX_CAPACITY: u64 = 100_000;

/// Build one of the per-verdict caches with the shared capacity and uniform TTL.
fn build_cache<V: Clone + Send + Sync + 'static>() -> Cache<String, V> {
    Cache::builder()
        .max_capacity(MAX_CAPACITY)
        .time_to_live(Duration::from_secs(CACHE_TTL_SECS))
        .build()
}

/// Caches blockchain permission check results behind a global generation.
pub struct BlockchainCache {
    /// `can_execute_action` results.
    execute_action: Cache<String, bool>,
    /// `can_use_wallet_in_action` results.
    use_wallet: Cache<String, bool>,
    /// `can_execute_action_and_use_wallet` results.
    execute_and_wallet: Cache<String, (bool, bool)>,
    /// `get_wallet_derivation` results.
    wallet_derivation: Cache<String, U256>,
    /// Global generation counter embedded in every cache key. Bumping it makes
    /// all existing keys unreachable (a full logical flush); old entries age out
    /// by TTL. A single atomic — no per-account bookkeeping.
    generation: AtomicU64,
}

impl BlockchainCache {
    fn new() -> Self {
        Self {
            execute_action: build_cache(),
            use_wallet: build_cache(),
            execute_and_wallet: build_cache(),
            wallet_derivation: build_cache(),
            generation: AtomicU64::new(0),
        }
    }

    /// Read the current global generation.
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Build a cache key for `can_execute_action`.
    pub fn execute_action_key(&self, api_key_hash: U256, cid_hash: U256) -> String {
        let g = self.generation();
        format!("{api_key_hash}:g{g}:{cid_hash}")
    }

    /// Build a cache key for `can_use_wallet_in_action`.
    pub fn use_wallet_key(&self, api_key_hash: U256, cid_hash: U256, wallet: Address) -> String {
        let g = self.generation();
        format!("{api_key_hash}:g{g}:{cid_hash}:{wallet:#x}")
    }

    /// Build a cache key for `can_execute_action_and_use_wallet`.
    pub fn execute_and_wallet_key(
        &self,
        api_key_hash: U256,
        cid_hash: U256,
        wallet: Address,
    ) -> String {
        let g = self.generation();
        format!("{api_key_hash}:g{g}:ew:{cid_hash}:{wallet:#x}")
    }

    /// Build a cache key for `get_wallet_derivation`.
    pub fn wallet_derivation_key(&self, api_key_hash: U256, wallet: Address) -> String {
        let g = self.generation();
        format!("{api_key_hash}:g{g}:wd:{wallet:#x}")
    }

    /// Reference to the `can_execute_action` cache.
    pub fn execute_action_cache(&self) -> &Cache<String, bool> {
        &self.execute_action
    }

    /// Reference to the `can_use_wallet_in_action` cache.
    pub fn use_wallet_cache(&self) -> &Cache<String, bool> {
        &self.use_wallet
    }

    /// Reference to the `can_execute_action_and_use_wallet` cache.
    pub fn execute_and_wallet_cache(&self) -> &Cache<String, (bool, bool)> {
        &self.execute_and_wallet
    }

    /// Reference to the `get_wallet_derivation` cache.
    pub fn wallet_derivation_cache(&self) -> &Cache<String, U256> {
        &self.wallet_derivation
    }

    /// Bump the global generation, logically flushing all cached verdicts.
    fn bump_generation(&self) {
        let next = self
            .generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        tracing::debug!("blockchain_cache: bumped global generation to {next}");
    }
}

static BLOCKCHAIN_CACHE_INSTANCE: OnceLock<BlockchainCache> = OnceLock::new();

/// Initialize the global blockchain cache. Call once during startup.
pub fn init() {
    BLOCKCHAIN_CACHE_INSTANCE.get_or_init(BlockchainCache::new);
    tracing::info!(
        "blockchain_cache: initialized (TTL={CACHE_TTL_SECS}s, global-generation invalidation)"
    );
}

/// Get the global cache instance. Returns `None` if not initialized.
pub fn get() -> Option<&'static BlockchainCache> {
    BLOCKCHAIN_CACHE_INSTANCE.get()
}

/// Invalidate the entire permission cache by bumping the global generation.
///
/// Called by the write-path mutation helpers in [`super`] (for instant
/// invalidation of mutations this process performs) and by the on-chain event
/// listener ([`crate::account_events`]) when it observes any permission-relevant
/// mutation. A no-op if the cache has not been initialized.
pub fn invalidate_all() {
    if let Some(cache) = get() {
        cache.bump_generation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> BlockchainCache {
        BlockchainCache::new()
    }

    fn addr_from_low_u64(n: u64) -> Address {
        let mut bytes = [0u8; 20];
        bytes[12..].copy_from_slice(&n.to_be_bytes());
        Address::from(bytes)
    }

    // ── Global generation counter ───────────────────────────────────

    #[test]
    fn generation_starts_at_zero() {
        let cache = test_cache();
        assert_eq!(cache.generation(), 0);
    }

    #[test]
    fn bump_generation_increments() {
        let cache = test_cache();
        cache.bump_generation();
        assert_eq!(cache.generation(), 1);
        cache.bump_generation();
        assert_eq!(cache.generation(), 2);
    }

    #[test]
    fn bump_generation_is_global() {
        // A single bump shifts the generation seen by *every* account's key.
        let cache = test_cache();
        let cid = U256::from(1u64);
        let key_a_before = cache.execute_action_key(U256::from(100u64), cid);
        let key_b_before = cache.execute_action_key(U256::from(200u64), cid);

        cache.bump_generation();

        let key_a_after = cache.execute_action_key(U256::from(100u64), cid);
        let key_b_after = cache.execute_action_key(U256::from(200u64), cid);
        assert_ne!(key_a_before, key_a_after);
        assert_ne!(key_b_before, key_b_after);
    }

    #[test]
    fn wrapping_add_at_u64_max() {
        let cache = test_cache();
        cache.generation.store(u64::MAX, Ordering::Relaxed);
        cache.bump_generation();
        assert_eq!(cache.generation(), 0);
    }

    // ── Key generation ──────────────────────────────────────────────

    #[test]
    fn execute_action_key_format() {
        let cache = test_cache();
        let key = cache.execute_action_key(U256::from(42u64), U256::from(99u64));
        assert_eq!(key, "42:g0:99");
    }

    #[test]
    fn use_wallet_key_format() {
        let cache = test_cache();
        let wallet = addr_from_low_u64(0xdead);
        let key = cache.use_wallet_key(U256::from(42u64), U256::from(99u64), wallet);
        assert!(key.starts_with("42:g0:99:"));
        assert!(key.contains("0x000000000000000000000000000000000000dead"));
    }

    #[test]
    fn execute_and_wallet_key_has_ew_discriminator() {
        let cache = test_cache();
        let key = cache.execute_and_wallet_key(U256::from(1u64), U256::from(2u64), Address::ZERO);
        assert!(
            key.contains(":ew:"),
            "key should contain :ew: discriminator, got: {key}"
        );
    }

    #[test]
    fn key_changes_after_bump() {
        let cache = test_cache();
        let hash = U256::from(42u64);
        let cid = U256::from(99u64);

        let key_before = cache.execute_action_key(hash, cid);
        assert!(key_before.contains(":g0:"));

        cache.bump_generation();

        let key_after = cache.execute_action_key(hash, cid);
        assert!(key_after.contains(":g1:"));
        assert_ne!(key_before, key_after);
    }

    // ── Cache hit / miss with generation ────────────────────────────

    #[tokio::test]
    async fn cache_hit_returns_stored_value() {
        let cache = test_cache();
        let hash = U256::from(10u64);
        let cid = U256::from(20u64);

        let key = cache.execute_action_key(hash, cid);
        cache.execute_action.insert(key.clone(), true).await;

        let key2 = cache.execute_action_key(hash, cid);
        assert_eq!(key, key2);
        assert_eq!(cache.execute_action.get(&key2).await, Some(true));
    }

    #[tokio::test]
    async fn cache_miss_after_invalidation() {
        let cache = test_cache();
        let hash = U256::from(10u64);
        let cid = U256::from(20u64);

        let key = cache.execute_action_key(hash, cid);
        cache.execute_action.insert(key.clone(), true).await;

        cache.bump_generation();

        // New key produces a miss; the old entry is unreachable (TTL-evicted later).
        let new_key = cache.execute_action_key(hash, cid);
        assert_ne!(key, new_key);
        assert_eq!(cache.execute_action.get(&new_key).await, None);
        assert_eq!(cache.execute_action.get(&key).await, Some(true));
    }

    #[tokio::test]
    async fn invalidation_is_global_across_accounts() {
        // One bump must invalidate every account, not just one.
        let cache = test_cache();
        let hash_a = U256::from(100u64);
        let hash_b = U256::from(200u64);
        let cid = U256::from(50u64);

        let key_a = cache.execute_action_key(hash_a, cid);
        let key_b = cache.execute_action_key(hash_b, cid);
        cache.execute_action.insert(key_a.clone(), true).await;
        cache.execute_action.insert(key_b.clone(), false).await;

        cache.bump_generation();

        let new_key_a = cache.execute_action_key(hash_a, cid);
        let new_key_b = cache.execute_action_key(hash_b, cid);
        assert_ne!(key_a, new_key_a);
        assert_ne!(key_b, new_key_b);
        assert_eq!(cache.execute_action.get(&new_key_a).await, None);
        assert_eq!(cache.execute_action.get(&new_key_b).await, None);
    }

    // ── use_wallet, execute_and_wallet, wallet_derivation caches ────

    #[tokio::test]
    async fn use_wallet_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(5u64);
        let cid = U256::from(6u64);
        let wallet = addr_from_low_u64(0xbeef);

        let key = cache.use_wallet_key(hash, cid, wallet);
        cache.use_wallet.insert(key.clone(), true).await;
        assert_eq!(cache.use_wallet.get(&key).await, Some(true));

        cache.bump_generation();
        let new_key = cache.use_wallet_key(hash, cid, wallet);
        assert_ne!(key, new_key);
        assert_eq!(cache.use_wallet.get(&new_key).await, None);
    }

    #[tokio::test]
    async fn execute_and_wallet_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(7u64);
        let cid = U256::from(8u64);
        let wallet = addr_from_low_u64(0xcafe);

        let key = cache.execute_and_wallet_key(hash, cid, wallet);
        cache
            .execute_and_wallet
            .insert(key.clone(), (true, false))
            .await;
        assert_eq!(
            cache.execute_and_wallet.get(&key).await,
            Some((true, false))
        );

        cache.bump_generation();
        let new_key = cache.execute_and_wallet_key(hash, cid, wallet);
        assert_eq!(cache.execute_and_wallet.get(&new_key).await, None);
    }

    #[test]
    fn wallet_derivation_key_has_wd_discriminator() {
        let cache = test_cache();
        let wallet = addr_from_low_u64(0xdead);
        let key = cache.wallet_derivation_key(U256::from(1u64), wallet);
        assert!(
            key.contains(":wd:"),
            "key should contain :wd: discriminator, got: {key}"
        );
    }

    #[tokio::test]
    async fn wallet_derivation_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(5u64);
        let wallet = addr_from_low_u64(0xbeef);

        let key = cache.wallet_derivation_key(hash, wallet);
        cache
            .wallet_derivation
            .insert(key.clone(), U256::from(42u64))
            .await;
        assert_eq!(
            cache.wallet_derivation.get(&key).await,
            Some(U256::from(42u64))
        );

        cache.bump_generation();
        let new_key = cache.wallet_derivation_key(hash, wallet);
        assert_ne!(key, new_key);
        assert_eq!(cache.wallet_derivation.get(&new_key).await, None);
    }

    // ── try_get_with integration ────────────────────────────────────

    #[tokio::test]
    async fn try_get_with_populates_on_miss() {
        let cache = test_cache();
        let key = cache.execute_action_key(U256::from(1u64), U256::from(2u64));

        let result = cache
            .execute_action
            .try_get_with(key.clone(), async { Ok::<_, anyhow::Error>(true) })
            .await
            .unwrap();
        assert!(result);

        let result2: bool = cache
            .execute_action
            .try_get_with(key, async {
                Err::<bool, anyhow::Error>(anyhow::anyhow!("should not be called on cache hit"))
            })
            .await
            .expect("should have been a cache hit");
        assert!(result2);
    }

    #[tokio::test]
    async fn try_get_with_misses_after_generation_bump() {
        let cache = test_cache();
        let hash = U256::from(1u64);
        let cid = U256::from(2u64);

        let key = cache.execute_action_key(hash, cid);
        cache
            .execute_action
            .try_get_with(key, async { Ok::<_, anyhow::Error>(true) })
            .await
            .unwrap();

        cache.bump_generation();

        let new_key = cache.execute_action_key(hash, cid);
        let mut closure_called = false;
        let result = cache
            .execute_action
            .try_get_with(new_key, async {
                closure_called = true;
                Ok::<_, anyhow::Error>(false)
            })
            .await
            .unwrap();
        assert!(
            closure_called,
            "closure should run on cache miss after bump"
        );
        assert!(!result, "should return the newly fetched value");
    }

    // ── invalidate_all (module-level) ────────────────────────────────

    #[test]
    fn invalidate_all_without_init_is_noop() {
        // Global INSTANCE may or may not be initialized depending on test order;
        // either way this must not panic.
        invalidate_all();
    }
}
