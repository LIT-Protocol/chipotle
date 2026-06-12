//! Global blockchain data cache.
//!
//! Caches the results of on-chain permission checks (`canExecuteAction`,
//! `canUseWalletInAction`) and wallet derivation lookups (`getWalletDerivation`)
//! so that repeated calls for the same API key and relevant parameters
//! avoid redundant contract calls.
//!
//! TTL: permission results use per-entry expiration — positive (authorized)
//! results live 60 minutes from insertion, while negative (denied) results
//! expire after 30 seconds. ChainSecured accounts mutate permissions by
//! sending wallet-signed transactions directly to the chain, which this
//! server never observes, so no invalidation hook can cover those writes; a
//! short negative TTL bounds how long a stale denial is served (previously a
//! newly group-permitted action returned 403 for up to an hour).
//! Wallet derivation lookups keep the flat 60-minute TTL.
//!
//! Invalidation uses a **per-account generation counter**: each API key hash
//! has an associated generation number embedded in the cache key. Bumping the
//! generation for an account causes all subsequent lookups to miss, while stale
//! entries with old generations are evicted naturally by TTL.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use alloy::primitives::{Address, U256};
use moka::Expiry;
use moka::future::Cache;

/// TTL in seconds for positive (authorized) permission results and wallet
/// derivations — 60 minutes.
const CACHE_TTL_SECS: u64 = 3600;

/// TTL in seconds for negative (denied) permission results. Kept short so
/// permissions granted out-of-band (ChainSecured wallet-signed transactions
/// submitted directly on-chain, or RPC state lag right after a grant) become
/// visible quickly, while still absorbing bursts of repeated unauthorized
/// requests between contract calls.
const NEGATIVE_CACHE_TTL_SECS: u64 = 30;

/// TTL for a permission result: full TTL when authorized, short when denied.
fn permission_ttl(authorized: bool) -> Duration {
    if authorized {
        Duration::from_secs(CACHE_TTL_SECS)
    } else {
        Duration::from_secs(NEGATIVE_CACHE_TTL_SECS)
    }
}

/// Per-entry expiry for `bool` permission caches (`can_execute_action`,
/// `can_use_wallet_in_action`): denials expire after `NEGATIVE_CACHE_TTL_SECS`.
struct PermissionExpiry;

impl Expiry<String, bool> for PermissionExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &bool,
        _created_at: Instant,
    ) -> Option<Duration> {
        Some(permission_ttl(*value))
    }

    fn expire_after_update(
        &self,
        _key: &String,
        value: &bool,
        _updated_at: Instant,
        _duration_until_expiry: Option<Duration>,
    ) -> Option<Duration> {
        Some(permission_ttl(*value))
    }
}

/// Per-entry expiry for the `(can_execute, can_use_wallet)` pair cache: the
/// request only proceeds when both are true, so any false in the pair is a
/// denial and expires after `NEGATIVE_CACHE_TTL_SECS`.
struct PairPermissionExpiry;

impl Expiry<String, (bool, bool)> for PairPermissionExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &(bool, bool),
        _created_at: Instant,
    ) -> Option<Duration> {
        Some(permission_ttl(value.0 && value.1))
    }

    fn expire_after_update(
        &self,
        _key: &String,
        value: &(bool, bool),
        _updated_at: Instant,
        _duration_until_expiry: Option<Duration>,
    ) -> Option<Duration> {
        Some(permission_ttl(value.0 && value.1))
    }
}

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
        // Permission caches use per-entry expiry (short TTL for denials).
        // The previous time_to_idle was redundant: with tti == ttl, the hard
        // time_to_live cap always fired first.
        let execute_action = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .expire_after(PermissionExpiry)
            .build();
        let use_wallet = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .expire_after(PermissionExpiry)
            .build();
        let execute_and_wallet = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .expire_after(PairPermissionExpiry)
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
    pub fn use_wallet_key(&self, api_key_hash: U256, cid_hash: U256, wallet: Address) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:{cid_hash}:{wallet:#x}")
    }

    /// Build a cache key for `can_execute_action_and_use_wallet`.
    pub fn execute_and_wallet_key(
        &self,
        api_key_hash: U256,
        cid_hash: U256,
        wallet: Address,
    ) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:ew:{cid_hash}:{wallet:#x}")
    }

    /// Build a cache key for `get_wallet_derivation`.
    pub fn wallet_derivation_key(&self, api_key_hash: U256, wallet: Address) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        format!("{h}:g{g}:wd:{wallet:#x}")
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
    tracing::info!(
        "blockchain_cache: initialized (TTL={CACHE_TTL_SECS}s, negative TTL={NEGATIVE_CACHE_TTL_SECS}s)"
    );
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
    match super::list_api_keys(api_key, U256::ZERO, U256::from(1000u64)).await {
        Ok(usage_keys) => {
            for uk in &usage_keys {
                let hash = uk.apiKeyHash.to_string();
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

    fn test_cache() -> BlockchainCache {
        BlockchainCache::new()
    }

    fn addr_from_low_u64(n: u64) -> Address {
        let mut bytes = [0u8; 20];
        bytes[12..].copy_from_slice(&n.to_be_bytes());
        Address::from(bytes)
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
        let hash = U256::from(42u64);
        let cid = U256::from(99u64);
        let key = cache.execute_action_key(hash, cid);
        assert_eq!(key, "42:g0:99");
    }

    #[test]
    fn use_wallet_key_format() {
        let cache = test_cache();
        let hash = U256::from(42u64);
        let cid = U256::from(99u64);
        let wallet = addr_from_low_u64(0xdead);
        let key = cache.use_wallet_key(hash, cid, wallet);
        assert!(key.starts_with("42:g0:99:"));
        assert!(key.contains("0x000000000000000000000000000000000000dead"));
    }

    #[test]
    fn execute_and_wallet_key_has_ew_discriminator() {
        let cache = test_cache();
        let hash = U256::from(1u64);
        let cid = U256::from(2u64);
        let wallet = Address::ZERO;
        let key = cache.execute_and_wallet_key(hash, cid, wallet);
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

        cache.bump_generation(&hash.to_string());

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

        // Same key should hit
        let key2 = cache.execute_action_key(hash, cid);
        assert_eq!(key, key2);
        assert_eq!(cache.execute_action.get(&key2).await, Some(true));
    }

    #[tokio::test]
    async fn cache_miss_after_invalidation() {
        let cache = test_cache();
        let hash = U256::from(10u64);
        let cid = U256::from(20u64);

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
        let hash_a = U256::from(100u64);
        let hash_b = U256::from(200u64);
        let cid = U256::from(50u64);

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
        let hash = U256::from(5u64);
        let cid = U256::from(6u64);
        let wallet = addr_from_low_u64(0xbeef);

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

        cache.bump_generation(&hash.to_string());
        let new_key = cache.execute_and_wallet_key(hash, cid, wallet);
        assert_eq!(cache.execute_and_wallet.get(&new_key).await, None);
    }

    // ── wallet_derivation cache ──────────────────────────────────────

    #[test]
    fn wallet_derivation_key_has_wd_discriminator() {
        let cache = test_cache();
        let hash = U256::from(1u64);
        let wallet = addr_from_low_u64(0xdead);
        let key = cache.wallet_derivation_key(hash, wallet);
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

        cache.bump_generation(&hash.to_string());
        let new_key = cache.wallet_derivation_key(hash, wallet);
        assert_ne!(key, new_key);
        assert_eq!(cache.wallet_derivation.get(&new_key).await, None);
    }

    // ── try_get_with integration ────────────────────────────────────

    #[tokio::test]
    async fn try_get_with_populates_on_miss() {
        let cache = test_cache();
        let hash = U256::from(1u64);
        let cid = U256::from(2u64);
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
        let hash = U256::from(1u64);
        let cid = U256::from(2u64);

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

    // ── Per-entry expiry policy ──────────────────────────────────────

    #[test]
    fn permission_ttl_is_full_for_authorized() {
        assert_eq!(
            permission_ttl(true),
            Duration::from_secs(CACHE_TTL_SECS),
            "authorized results should keep the full TTL"
        );
    }

    #[test]
    fn permission_ttl_is_short_for_denied() {
        assert_eq!(
            permission_ttl(false),
            Duration::from_secs(NEGATIVE_CACHE_TTL_SECS),
            "denied results should expire quickly so out-of-band on-chain \
             grants (ChainSecured) are picked up without waiting out the TTL"
        );
    }

    #[test]
    fn permission_expiry_create_uses_value() {
        let now = Instant::now();
        let key = "k".to_string();
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &true, now),
            Some(Duration::from_secs(CACHE_TTL_SECS))
        );
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &false, now),
            Some(Duration::from_secs(NEGATIVE_CACHE_TTL_SECS))
        );
    }

    #[test]
    fn permission_expiry_update_uses_new_value() {
        let now = Instant::now();
        let key = "k".to_string();
        // A denial overwritten by a grant should adopt the full TTL, and vice versa.
        assert_eq!(
            PermissionExpiry.expire_after_update(
                &key,
                &true,
                now,
                Some(Duration::from_secs(NEGATIVE_CACHE_TTL_SECS))
            ),
            Some(Duration::from_secs(CACHE_TTL_SECS))
        );
        assert_eq!(
            PermissionExpiry.expire_after_update(
                &key,
                &false,
                now,
                Some(Duration::from_secs(CACHE_TTL_SECS))
            ),
            Some(Duration::from_secs(NEGATIVE_CACHE_TTL_SECS))
        );
    }

    #[test]
    fn pair_permission_expiry_short_unless_both_true() {
        let now = Instant::now();
        let key = "k".to_string();
        let full = Some(Duration::from_secs(CACHE_TTL_SECS));
        let short = Some(Duration::from_secs(NEGATIVE_CACHE_TTL_SECS));
        assert_eq!(
            PairPermissionExpiry.expire_after_create(&key, &(true, true), now),
            full
        );
        assert_eq!(
            PairPermissionExpiry.expire_after_create(&key, &(true, false), now),
            short
        );
        assert_eq!(
            PairPermissionExpiry.expire_after_create(&key, &(false, true), now),
            short
        );
        assert_eq!(
            PairPermissionExpiry.expire_after_create(&key, &(false, false), now),
            short
        );
        assert_eq!(
            PairPermissionExpiry.expire_after_update(&key, &(true, true), now, short),
            full
        );
    }

    #[tokio::test]
    async fn denied_entry_expires_after_negative_ttl() {
        // Build a cache with a sub-second negative TTL surrogate to verify the
        // expire_after wiring end-to-end without waiting 30 real seconds.
        struct FastExpiry;
        impl Expiry<String, bool> for FastExpiry {
            fn expire_after_create(
                &self,
                _key: &String,
                value: &bool,
                _created_at: Instant,
            ) -> Option<Duration> {
                Some(if *value {
                    Duration::from_secs(3600)
                } else {
                    Duration::from_millis(50)
                })
            }
        }
        let cache: Cache<String, bool> = Cache::builder().expire_after(FastExpiry).build();
        cache.insert("denied".to_string(), false).await;
        cache.insert("granted".to_string(), true).await;
        assert_eq!(cache.get(&"denied".to_string()).await, Some(false));

        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(
            cache.get(&"denied".to_string()).await,
            None,
            "denied entry should expire after the negative TTL"
        );
        assert_eq!(
            cache.get(&"granted".to_string()).await,
            Some(true),
            "granted entry should still be cached"
        );
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
