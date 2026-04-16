//! Global blockchain data cache.
//!
//! Caches the results of on-chain permission checks (`canExecuteAction`,
//! `canUseWalletInAction`) and wallet derivation lookups (`getWalletDerivation`)
//! so that repeated calls for the same API key and action/wallet combination
//! avoid redundant contract calls.
//!
//! TTL: 60 minutes idle (extending on every access) with a hard upper bound
//! of 60 minutes from insertion (`time_to_live`), ensuring entries are never
//! served beyond the TTL even under continuous traffic.
//!
//! Invalidation uses a **per-account generation counter**: each API key hash
//! has an associated generation number embedded in the cache key. Bumping the
//! generation for an account causes all subsequent lookups to miss, while stale
//! entries with old generations are evicted naturally by TTL.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use ethers::types::U256;
use moka::future::Cache;

/// TTL in seconds — 60 minutes, extending on every access.
const CACHE_TTL_SECS: u64 = 3600;

/// Maximum entries per cache.
const MAX_CAPACITY: u64 = 100_000;

/// Caches blockchain permission check results with per-account invalidation.
pub struct BlockchainCache {
    /// `can_execute_action` results.
    execute_action: Cache<String, bool>,
    /// `can_use_wallet_in_action` results.
    use_wallet: Cache<String, bool>,
    /// `can_execute_action_and_use_wallet` results.
    execute_and_wallet: Cache<String, (bool, bool)>,
    /// `get_wallet_derivation` results.
    wallet_derivation: Cache<String, U256>,
    /// Per-account generation counter keyed by the string representation of
    /// the api_key_hash (`U256`). Uses a plain HashMap (no eviction) to
    /// guarantee that a bumped generation is never lost. Each entry is ~100
    /// bytes; even 100k accounts is only ~10MB.
    generations: RwLock<HashMap<String, u64>>,
}

impl BlockchainCache {
    fn new() -> Self {
        let ttl = Duration::from_secs(CACHE_TTL_SECS);
        let execute_action = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .time_to_idle(ttl)
            .time_to_live(ttl)
            .build();
        let use_wallet = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .time_to_idle(ttl)
            .time_to_live(ttl)
            .build();
        let execute_and_wallet = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .time_to_idle(ttl)
            .time_to_live(ttl)
            .build();
        let wallet_derivation = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .time_to_idle(ttl)
            .time_to_live(ttl)
            .build();
        Self {
            execute_action,
            use_wallet,
            execute_and_wallet,
            wallet_derivation,
            generations: RwLock::new(HashMap::new()),
        }
    }

    /// Read the current generation for an api_key_hash. Returns 0 if unseen.
    fn generation(&self, api_key_hash: &str) -> u64 {
        self.generations
            .read()
            .expect("generation lock poisoned")
            .get(api_key_hash)
            .copied()
            .unwrap_or(0)
    }

    /// Build a cache key for `can_execute_action`.
    pub fn execute_action_key(&self, api_key_hash: U256, cid_hash: U256) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:{cid_hash}")
    }

    /// Build a cache key for `can_use_wallet_in_action`.
    pub fn use_wallet_key(
        &self,
        api_key_hash: U256,
        cid_hash: U256,
        wallet: ethers::types::H160,
    ) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:{cid_hash}:{wallet:?}")
    }

    /// Build a cache key for `can_execute_action_and_use_wallet`.
    pub fn execute_and_wallet_key(
        &self,
        api_key_hash: U256,
        cid_hash: U256,
        wallet: ethers::types::H160,
    ) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:ew:{cid_hash}:{wallet:?}")
    }

    /// Build a cache key for `get_wallet_derivation`.
    pub fn wallet_derivation_key(
        &self,
        api_key_hash: U256,
        wallet: ethers::types::H160,
    ) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:wd:{wallet:?}")
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

    /// Bump the generation for a single api_key_hash, invalidating all cached
    /// permission entries for that key.
    fn bump_generation(&self, api_key_hash: &str) {
        let mut gens = self.generations.write().expect("generation lock poisoned");
        let entry = gens.entry(api_key_hash.to_string()).or_insert(0);
        *entry = entry.wrapping_add(1);
        let next = *entry;
        tracing::debug!(
            "blockchain_cache: bumped generation for {} to {}",
            api_key_hash,
            next
        );
    }
}

static BLOCKCHAIN_CACHE_INSTANCE: OnceLock<BlockchainCache> = OnceLock::new();

/// Initialize the global blockchain cache. Call once during startup.
pub fn init() {
    BLOCKCHAIN_CACHE_INSTANCE.get_or_init(BlockchainCache::new);
    tracing::info!("blockchain_cache: initialized (TTL={CACHE_TTL_SECS}s)");
}

/// Get the global cache instance. Returns `None` if not initialized.
pub fn get() -> Option<&'static BlockchainCache> {
    BLOCKCHAIN_CACHE_INSTANCE.get()
}

/// Invalidate cached permission entries for the given API key.
///
/// Prefer `invalidate_for_account` for group/action/PKP mutations, which also
/// invalidates usage keys under the same account.
pub fn invalidate_for_key(api_key: &str) {
    if let Some(cache) = get() {
        let hash = crate::utils::parse_with_hash::api_key_hash(api_key).to_string();
        cache.bump_generation(&hash);
    }
}

/// Invalidate cached permission entries for an entire account: the calling key
/// and all usage keys returned by `list_api_keys`.
///
/// Fetches usage key hashes via a chain call to `list_api_keys`. Call after
/// group/action/PKP mutations where any key under the account could be affected.
///
/// **Limitation:** If the caller authenticates with a usage key (not the master
/// key), the master key's cached entries are NOT invalidated here because the
/// contract does not expose a `resolveToMaster` view. In that case the master
/// key's entries expire naturally via the 60-minute `time_to_live`. This is
/// acceptable because usage-key-driven management mutations are uncommon in
/// practice.
pub async fn invalidate_for_account(api_key: &str) {
    let Some(cache) = get() else { return };

    // Always bump the calling key (master or usage).
    let caller_hash = crate::utils::parse_with_hash::api_key_hash(api_key).to_string();
    cache.bump_generation(&caller_hash);

    // Fetch all usage keys under this account and bump each one.
    // list_api_keys resolves both master and usage keys to the correct account.
    match super::list_api_keys(api_key, U256::zero(), U256::from(1000)).await {
        Ok(usage_keys) => {
            for uk in &usage_keys {
                let hash = uk.api_key_hash.to_string();
                if hash != caller_hash {
                    cache.bump_generation(&hash);
                }
            }
            tracing::debug!(
                "blockchain_cache: invalidated account ({} usage keys)",
                usage_keys.len()
            );
        }
        Err(e) => {
            tracing::warn!(
                "blockchain_cache: failed to list usage keys for invalidation: {e}. \
                 Usage key cache entries may be stale until TTL."
            );
        }
    }
}

/// Invalidate cached permission entries for both a master key and a usage key.
///
/// Call after usage-API-key mutations where both keys' cached entries may be stale.
/// Uses `usage_api_key_to_hash` for the usage key to handle both raw keys and
/// pre-computed hashes consistently with the on-chain mutation path.
pub fn invalidate_for_keys(master_api_key: &str, usage_api_key: &str) {
    if let Some(cache) = get() {
        let master_hash = crate::utils::parse_with_hash::api_key_hash(master_api_key).to_string();
        let usage_hash =
            crate::utils::parse_with_hash::usage_api_key_to_hash(usage_api_key).to_string();
        cache.bump_generation(&master_hash);
        if usage_hash != master_hash {
            cache.bump_generation(&usage_hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::H160;

    fn test_cache() -> BlockchainCache {
        BlockchainCache::new()
    }

    // ── Generation counter ──────────────────────────────────────────

    #[test]
    fn generation_starts_at_zero() {
        let cache = test_cache();
        assert_eq!(cache.generation("anything"), 0);
    }

    #[test]
    fn bump_generation_increments() {
        let cache = test_cache();
        cache.bump_generation("key1");
        assert_eq!(cache.generation("key1"), 1);
        cache.bump_generation("key1");
        assert_eq!(cache.generation("key1"), 2);
    }

    #[test]
    fn bump_generation_is_per_key() {
        let cache = test_cache();
        cache.bump_generation("key_a");
        cache.bump_generation("key_a");
        cache.bump_generation("key_b");
        assert_eq!(cache.generation("key_a"), 2);
        assert_eq!(cache.generation("key_b"), 1);
        assert_eq!(cache.generation("key_c"), 0);
    }

    #[test]
    fn wrapping_add_at_u64_max() {
        let cache = test_cache();
        cache
            .generations
            .write()
            .unwrap()
            .insert("overflow".to_string(), u64::MAX);
        cache.bump_generation("overflow");
        assert_eq!(cache.generation("overflow"), 0);
    }

    // ── Key generation ──────────────────────────────────────────────

    #[test]
    fn execute_action_key_format() {
        let cache = test_cache();
        let hash = U256::from(42);
        let cid = U256::from(99);
        let key = cache.execute_action_key(hash, cid);
        assert_eq!(key, "42:g0:99");
    }

    #[test]
    fn use_wallet_key_format() {
        let cache = test_cache();
        let hash = U256::from(42);
        let cid = U256::from(99);
        let wallet = H160::from_low_u64_be(0xdead);
        let key = cache.use_wallet_key(hash, cid, wallet);
        assert!(key.starts_with("42:g0:99:"));
        assert!(key.contains("0x000000000000000000000000000000000000dead"));
    }

    #[test]
    fn execute_and_wallet_key_has_ew_discriminator() {
        let cache = test_cache();
        let hash = U256::from(1);
        let cid = U256::from(2);
        let wallet = H160::zero();
        let key = cache.execute_and_wallet_key(hash, cid, wallet);
        assert!(
            key.contains(":ew:"),
            "key should contain :ew: discriminator, got: {key}"
        );
    }

    #[test]
    fn key_changes_after_bump() {
        let cache = test_cache();
        let hash = U256::from(42);
        let cid = U256::from(99);

        let key_before = cache.execute_action_key(hash, cid);
        assert!(key_before.contains(":g0:"));

        cache.bump_generation(&hash.to_string());

        let key_after = cache.execute_action_key(hash, cid);
        assert!(key_after.contains(":g1:"));
        assert_ne!(key_before, key_after);
    }

    // ── Cache hit / miss with generation ────────────────────────────

    #[tokio::test]
    async fn cache_hit_returns_stored_value() {
        let cache = test_cache();
        let hash = U256::from(10);
        let cid = U256::from(20);

        let key = cache.execute_action_key(hash, cid);
        cache.execute_action.insert(key.clone(), true).await;

        // Same key should hit
        let key2 = cache.execute_action_key(hash, cid);
        assert_eq!(key, key2);
        assert_eq!(cache.execute_action.get(&key2).await, Some(true));
    }

    #[tokio::test]
    async fn cache_miss_after_invalidation() {
        let cache = test_cache();
        let hash = U256::from(10);
        let cid = U256::from(20);

        // Populate cache
        let key = cache.execute_action_key(hash, cid);
        cache.execute_action.insert(key.clone(), true).await;

        // Bump generation
        cache.bump_generation(&hash.to_string());

        // New key should be different — cache miss
        let new_key = cache.execute_action_key(hash, cid);
        assert_ne!(key, new_key);
        assert_eq!(cache.execute_action.get(&new_key).await, None);

        // Old key entry still exists (evicted by TTL later)
        assert_eq!(cache.execute_action.get(&key).await, Some(true));
    }

    #[tokio::test]
    async fn invalidation_is_per_account() {
        let cache = test_cache();
        let hash_a = U256::from(100);
        let hash_b = U256::from(200);
        let cid = U256::from(50);

        // Populate both accounts
        let key_a = cache.execute_action_key(hash_a, cid);
        let key_b = cache.execute_action_key(hash_b, cid);
        cache.execute_action.insert(key_a.clone(), true).await;
        cache.execute_action.insert(key_b.clone(), false).await;

        // Invalidate only account A
        cache.bump_generation(&hash_a.to_string());

        // Account A key changed (miss)
        let new_key_a = cache.execute_action_key(hash_a, cid);
        assert_ne!(key_a, new_key_a);
        assert_eq!(cache.execute_action.get(&new_key_a).await, None);

        // Account B key unchanged (still hits)
        let new_key_b = cache.execute_action_key(hash_b, cid);
        assert_eq!(key_b, new_key_b);
        assert_eq!(cache.execute_action.get(&new_key_b).await, Some(false));
    }

    // ── use_wallet and execute_and_wallet caches ────────────────────

    #[tokio::test]
    async fn use_wallet_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(5);
        let cid = U256::from(6);
        let wallet = H160::from_low_u64_be(0xbeef);

        let key = cache.use_wallet_key(hash, cid, wallet);
        cache.use_wallet.insert(key.clone(), true).await;
        assert_eq!(cache.use_wallet.get(&key).await, Some(true));

        cache.bump_generation(&hash.to_string());
        let new_key = cache.use_wallet_key(hash, cid, wallet);
        assert_ne!(key, new_key);
        assert_eq!(cache.use_wallet.get(&new_key).await, None);
    }

    #[tokio::test]
    async fn execute_and_wallet_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(7);
        let cid = U256::from(8);
        let wallet = H160::from_low_u64_be(0xcafe);

        let key = cache.execute_and_wallet_key(hash, cid, wallet);
        cache
            .execute_and_wallet
            .insert(key.clone(), (true, false))
            .await;
        assert_eq!(
            cache.execute_and_wallet.get(&key).await,
            Some((true, false))
        );

        cache.bump_generation(&hash.to_string());
        let new_key = cache.execute_and_wallet_key(hash, cid, wallet);
        assert_eq!(cache.execute_and_wallet.get(&new_key).await, None);
    }

    // ── wallet_derivation cache ──────────────────────────────────────

    #[test]
    fn wallet_derivation_key_has_wd_discriminator() {
        let cache = test_cache();
        let hash = U256::from(1);
        let wallet = H160::from_low_u64_be(0xdead);
        let key = cache.wallet_derivation_key(hash, wallet);
        assert!(
            key.contains(":wd:"),
            "key should contain :wd: discriminator, got: {key}"
        );
    }

    #[tokio::test]
    async fn wallet_derivation_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(5);
        let wallet = H160::from_low_u64_be(0xbeef);

        let key = cache.wallet_derivation_key(hash, wallet);
        cache
            .wallet_derivation
            .insert(key.clone(), U256::from(42))
            .await;
        assert_eq!(
            cache.wallet_derivation.get(&key).await,
            Some(U256::from(42))
        );

        cache.bump_generation(&hash.to_string());
        let new_key = cache.wallet_derivation_key(hash, wallet);
        assert_ne!(key, new_key);
        assert_eq!(cache.wallet_derivation.get(&new_key).await, None);
    }

    // ── try_get_with integration ────────────────────────────────────

    #[tokio::test]
    async fn try_get_with_populates_on_miss() {
        let cache = test_cache();
        let hash = U256::from(1);
        let cid = U256::from(2);
        let key = cache.execute_action_key(hash, cid);

        // Cache miss triggers the closure
        let result = cache
            .execute_action
            .try_get_with(key.clone(), async { Ok::<_, anyhow::Error>(true) })
            .await
            .unwrap();
        assert!(result);

        // Second call should hit the cache (no closure needed)
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
        let hash = U256::from(1);
        let cid = U256::from(2);

        // Populate via try_get_with
        let key = cache.execute_action_key(hash, cid);
        cache
            .execute_action
            .try_get_with(key, async { Ok::<_, anyhow::Error>(true) })
            .await
            .unwrap();

        // Bump generation
        cache.bump_generation(&hash.to_string());

        // New key produces a miss, closure runs
        let new_key = cache.execute_action_key(hash, cid);
        let mut closure_called = false;
        let result = cache
            .execute_action
            .try_get_with(new_key, async {
                closure_called = true;
                Ok::<_, anyhow::Error>(false) // different value proves it re-fetched
            })
            .await
            .unwrap();
        assert!(
            closure_called,
            "closure should run on cache miss after bump"
        );
        assert!(!result, "should return the newly fetched value");
    }

    // ── invalidate_for_key / invalidate_for_keys (module-level) ─────

    #[test]
    fn invalidate_for_key_without_init_is_noop() {
        // Global INSTANCE not initialized in test — should not panic
        // (We can't test the initialized path without polluting the global,
        // but we verify the None path is safe.)
        // Note: if init() was called by another test in the same process,
        // this would actually bump. That's fine — we just verify no panic.
        invalidate_for_key("some_api_key");
    }

    #[test]
    fn invalidate_for_keys_without_init_is_noop() {
        invalidate_for_keys("master", "usage");
    }
}
