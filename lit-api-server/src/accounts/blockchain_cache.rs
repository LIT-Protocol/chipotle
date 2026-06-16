//! Global blockchain data cache.
//!
//! Caches the results of on-chain permission checks (`canExecuteAction`,
//! `canUseWalletInAction`) and wallet derivation lookups (`getWalletDerivation`)
//! so that repeated calls for the same API key and relevant parameters avoid
//! redundant contract calls.
//!
//! # Design: verdict memoizer with per-account invalidation
//!
//! The source of truth for permissions is the chain. This cache is *only* a
//! performance optimization in front of it. It memoizes opaque contract verdicts
//! keyed by `(api_key_hash, params)`, exactly as the contract resolves them. A
//! cache miss simply asks the chain.
//!
//! ## Invalidation: one generation counter per account
//!
//! Every cache key embeds a generation number for the account it belongs to.
//! Bumping that account's generation makes all of its subsequent lookups miss
//! (and the stale entries age out by TTL), while leaving every *other* account's
//! entries untouched. So a mutation to account A flushes only A — never the
//! whole cache.
//!
//! The account is identified by its **wallet address**, not by the calling key.
//! This is the crux: an account has one master key and many usage keys, each
//! with independent scope, so verdicts are cached per *calling* key. But a
//! group/action/PKP mutation changes the verdict for *every* key under the
//! account, and the on-chain event carries only the master `apiKeyHash`. Both
//! the master and every usage key resolve (via `getAccountWalletAddress`) to the
//! same wallet, so we key the generation by that wallet: bumping it once
//! invalidates the master and all usage keys at once, with no need to enumerate
//! the usage keys. The wallet for a calling key is resolved lazily (and memoized
//! in [`resolution_cache`]) when its cache key is built; see
//! [`super::resolve_account_wallet_hash`].
//!
//! ## Driven by events and write-path hooks
//!
//! Generations are bumped by [`super::invalidate_account_by_hash`], called from
//! the on-chain event listener ([`crate::account_events`]) for any
//! permission-relevant mutation, and from the write-path hooks in [`super`] for
//! mutations this process performs (instant, no poll lag). The listener also
//! covers mutations this process never sees — ChainSecured wallet-signed
//! transactions sent directly to the contract, or mutations by another replica.
//!
//! ## TTL: a backstop, not the freshness mechanism
//!
//! Invalidation keeps the cache fresh; the TTL is only insurance for the windows
//! invalidation can't cover (a missed-block gap, an ownership transfer that moves
//! the wallet before the resolution entry expires, a replica between boot and its
//! first poll). Denials use a much shorter TTL than grants
//! ([`NEGATIVE_CACHE_TTL_SECS`] vs [`CACHE_TTL_SECS`]) so that a permission
//! granted while the listener is lagging or down still becomes visible quickly,
//! rather than a stale denial being honored for the full grant TTL.
//!
//! ## Stateless / horizontally scalable by construction
//!
//! The generation map is process-local. Each replica runs its own listener and
//! bumps its own generations off the same chain logs, so replicas converge
//! independently with no shared state. A freshly booted replica starts with an
//! empty cache (everything misses → everything is fresh) and a listener anchored
//! at the current block; it never needs history.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use alloy::primitives::{Address, U256};
use moka::Expiry;
use moka::future::Cache;

/// TTL for positive (authorized) verdicts and for non-permission caches
/// (wallet derivation, hash→wallet resolution). A backstop for the windows
/// invalidation can't cover; invalidation, not expiry, is the primary freshness
/// mechanism, so this is generous.
const CACHE_TTL_SECS: u64 = 300;

/// TTL for negative (denied) permission verdicts. Kept short so a permission
/// granted out-of-band (a ChainSecured wallet-signed transaction, or RPC state
/// lag right after a grant) becomes visible quickly even if the event listener
/// is lagging or down — without it, a fresh denial would be honored for the full
/// [`CACHE_TTL_SECS`]. Bounds the freshly-granted-but-still-denied window.
const NEGATIVE_CACHE_TTL_SECS: u64 = 30;

/// TTL for the `api_key_hash → account wallet` resolution cache. The mapping is
/// stable except on ownership transfer, so this can be generous; it bounds how
/// long a transferred account keeps composing its old wallet's generation.
const RESOLUTION_TTL_SECS: u64 = 300;

/// Maximum entries per cache.
const MAX_CAPACITY: u64 = 100_000;

/// TTL for a permission verdict: full when authorized, short when denied.
fn permission_ttl(authorized: bool) -> Duration {
    if authorized {
        Duration::from_secs(CACHE_TTL_SECS)
    } else {
        Duration::from_secs(NEGATIVE_CACHE_TTL_SECS)
    }
}

/// Per-entry expiry for the `bool` permission caches (`can_execute_action`,
/// `can_use_wallet_in_action`): denials expire after [`NEGATIVE_CACHE_TTL_SECS`],
/// grants after [`CACHE_TTL_SECS`].
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
/// request only proceeds when both are true, so any `false` in the pair is a
/// denial and expires after [`NEGATIVE_CACHE_TTL_SECS`].
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

/// Build a cache with the shared capacity and full positive TTL (used for the
/// non-permission caches; permission caches use a per-entry [`Expiry`] instead).
fn build_cache<V: Clone + Send + Sync + 'static>() -> Cache<String, V> {
    Cache::builder()
        .max_capacity(MAX_CAPACITY)
        .time_to_live(Duration::from_secs(CACHE_TTL_SECS))
        .build()
}

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
    /// Memoized `api_key_hash → account wallet` resolutions. The loader (a
    /// `getAccountWalletAddress` chain call) lives in [`super`]; this just holds
    /// the results so the hot path resolves at most once per key per TTL.
    resolution: Cache<String, Address>,
    /// Per-account generation counter keyed by account wallet address. A plain
    /// HashMap (no eviction) so a bumped generation is never lost. Each entry is
    /// tiny; even 100k accounts is a few MB.
    account_generations: RwLock<HashMap<Address, u64>>,
}

impl BlockchainCache {
    fn new() -> Self {
        // Permission caches use per-entry expiry so denials expire fast; the
        // non-permission caches use the plain positive TTL.
        Self {
            execute_action: Cache::builder()
                .max_capacity(MAX_CAPACITY)
                .expire_after(PermissionExpiry)
                .build(),
            use_wallet: Cache::builder()
                .max_capacity(MAX_CAPACITY)
                .expire_after(PermissionExpiry)
                .build(),
            execute_and_wallet: Cache::builder()
                .max_capacity(MAX_CAPACITY)
                .expire_after(PairPermissionExpiry)
                .build(),
            wallet_derivation: build_cache(),
            resolution: Cache::builder()
                .max_capacity(MAX_CAPACITY)
                .time_to_live(Duration::from_secs(RESOLUTION_TTL_SECS))
                .build(),
            account_generations: RwLock::new(HashMap::new()),
        }
    }

    /// Read the current generation for an account wallet. Returns 0 if unseen.
    pub fn account_generation(&self, wallet: Address) -> u64 {
        self.account_generations
            .read()
            .expect("account generation lock poisoned")
            .get(&wallet)
            .copied()
            .unwrap_or(0)
    }

    /// Bump the generation for an account wallet, invalidating every cached
    /// verdict (master and all usage keys) under that account.
    pub fn bump_account_generation(&self, wallet: Address) {
        let mut gens = self
            .account_generations
            .write()
            .expect("account generation lock poisoned");
        let entry = gens.entry(wallet).or_insert(0);
        *entry = entry.wrapping_add(1);
        tracing::debug!(
            "blockchain_cache: bumped generation for account {wallet:#x} to {}",
            *entry
        );
    }

    /// Build a cache key for `can_execute_action`. `account_gen` is the caller's
    /// account generation (see [`account_generation`]).
    pub fn execute_action_key(
        &self,
        api_key_hash: U256,
        account_gen: u64,
        cid_hash: U256,
    ) -> String {
        format!("{api_key_hash}:a{account_gen}:{cid_hash}")
    }

    /// Build a cache key for `can_use_wallet_in_action`.
    pub fn use_wallet_key(
        &self,
        api_key_hash: U256,
        account_gen: u64,
        cid_hash: U256,
        wallet: Address,
    ) -> String {
        format!("{api_key_hash}:a{account_gen}:{cid_hash}:{wallet:#x}")
    }

    /// Build a cache key for `can_execute_action_and_use_wallet`.
    pub fn execute_and_wallet_key(
        &self,
        api_key_hash: U256,
        account_gen: u64,
        cid_hash: U256,
        wallet: Address,
    ) -> String {
        format!("{api_key_hash}:a{account_gen}:ew:{cid_hash}:{wallet:#x}")
    }

    /// Build a cache key for `get_wallet_derivation`.
    pub fn wallet_derivation_key(
        &self,
        api_key_hash: U256,
        account_gen: u64,
        wallet: Address,
    ) -> String {
        format!("{api_key_hash}:a{account_gen}:wd:{wallet:#x}")
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

    /// Reference to the `api_key_hash → account wallet` resolution cache.
    pub fn resolution_cache(&self) -> &Cache<String, Address> {
        &self.resolution
    }
}

static BLOCKCHAIN_CACHE_INSTANCE: OnceLock<BlockchainCache> = OnceLock::new();

/// Initialize the global blockchain cache. Call once during startup.
pub fn init() {
    BLOCKCHAIN_CACHE_INSTANCE.get_or_init(BlockchainCache::new);
    tracing::info!(
        "blockchain_cache: initialized (TTL={CACHE_TTL_SECS}s, negative TTL={NEGATIVE_CACHE_TTL_SECS}s, per-account-generation invalidation)"
    );
}

/// Get the global cache instance. Returns `None` if not initialized.
pub fn get() -> Option<&'static BlockchainCache> {
    BLOCKCHAIN_CACHE_INSTANCE.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> BlockchainCache {
        BlockchainCache::new()
    }

    // ── Per-entry expiry policy ──────────────────────────────────────

    #[test]
    fn permission_ttl_full_for_authorized_short_for_denied() {
        assert_eq!(permission_ttl(true), Duration::from_secs(CACHE_TTL_SECS));
        assert_eq!(
            permission_ttl(false),
            Duration::from_secs(NEGATIVE_CACHE_TTL_SECS)
        );
        assert!(NEGATIVE_CACHE_TTL_SECS < CACHE_TTL_SECS);
    }

    #[test]
    fn permission_expiry_uses_value() {
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
    }

    #[tokio::test]
    async fn denied_entry_expires_before_granted() {
        // End-to-end wiring check with a sub-second surrogate so we don't wait
        // 30 real seconds: a denial should evict while a grant survives.
        struct FastExpiry;
        impl Expiry<String, bool> for FastExpiry {
            fn expire_after_create(&self, _k: &String, v: &bool, _c: Instant) -> Option<Duration> {
                Some(if *v {
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
        assert_eq!(cache.get(&"denied".to_string()).await, None);
        assert_eq!(cache.get(&"granted".to_string()).await, Some(true));
    }

    fn wallet_from_low_u64(n: u64) -> Address {
        let mut bytes = [0u8; 20];
        bytes[12..].copy_from_slice(&n.to_be_bytes());
        Address::from(bytes)
    }

    fn addr_from_low_u64(n: u64) -> Address {
        wallet_from_low_u64(n)
    }

    // ── Per-account generation counter ──────────────────────────────

    #[test]
    fn generation_starts_at_zero() {
        let cache = test_cache();
        assert_eq!(cache.account_generation(wallet_from_low_u64(1)), 0);
    }

    #[test]
    fn bump_generation_increments() {
        let cache = test_cache();
        let w = wallet_from_low_u64(1);
        cache.bump_account_generation(w);
        assert_eq!(cache.account_generation(w), 1);
        cache.bump_account_generation(w);
        assert_eq!(cache.account_generation(w), 2);
    }

    #[test]
    fn bump_generation_is_per_account() {
        let cache = test_cache();
        let a = wallet_from_low_u64(0xaaaa);
        let b = wallet_from_low_u64(0xbbbb);
        cache.bump_account_generation(a);
        cache.bump_account_generation(a);
        cache.bump_account_generation(b);
        assert_eq!(cache.account_generation(a), 2);
        assert_eq!(cache.account_generation(b), 1);
        assert_eq!(cache.account_generation(wallet_from_low_u64(0xcccc)), 0);
    }

    #[test]
    fn wrapping_add_at_u64_max() {
        let cache = test_cache();
        let w = wallet_from_low_u64(7);
        cache
            .account_generations
            .write()
            .unwrap()
            .insert(w, u64::MAX);
        cache.bump_account_generation(w);
        assert_eq!(cache.account_generation(w), 0);
    }

    // ── Key generation ──────────────────────────────────────────────

    #[test]
    fn execute_action_key_format() {
        let cache = test_cache();
        let key = cache.execute_action_key(U256::from(42u64), 0, U256::from(99u64));
        assert_eq!(key, "42:a0:99");
    }

    #[test]
    fn use_wallet_key_format() {
        let cache = test_cache();
        let wallet = addr_from_low_u64(0xdead);
        let key = cache.use_wallet_key(U256::from(42u64), 0, U256::from(99u64), wallet);
        assert!(key.starts_with("42:a0:99:"));
        assert!(key.contains("0x000000000000000000000000000000000000dead"));
    }

    #[test]
    fn execute_and_wallet_key_has_ew_discriminator() {
        let cache = test_cache();
        let key =
            cache.execute_and_wallet_key(U256::from(1u64), 0, U256::from(2u64), Address::ZERO);
        assert!(
            key.contains(":ew:"),
            "key should contain :ew: discriminator, got: {key}"
        );
    }

    #[test]
    fn wallet_derivation_key_has_wd_discriminator() {
        let cache = test_cache();
        let wallet = addr_from_low_u64(0xdead);
        let key = cache.wallet_derivation_key(U256::from(1u64), 0, wallet);
        assert!(
            key.contains(":wd:"),
            "key should contain :wd: discriminator, got: {key}"
        );
    }

    #[test]
    fn key_embeds_account_generation() {
        let cache = test_cache();
        let hash = U256::from(42u64);
        let cid = U256::from(99u64);
        let before = cache.execute_action_key(hash, 0, cid);
        let after = cache.execute_action_key(hash, 1, cid);
        assert!(before.contains(":a0:"));
        assert!(after.contains(":a1:"));
        assert_ne!(before, after);
    }

    // ── Cache hit / miss with generation ────────────────────────────

    #[tokio::test]
    async fn cache_hit_returns_stored_value() {
        let cache = test_cache();
        let key = cache.execute_action_key(U256::from(10u64), 0, U256::from(20u64));
        cache.execute_action.insert(key.clone(), true).await;
        assert_eq!(cache.execute_action.get(&key).await, Some(true));
    }

    #[tokio::test]
    async fn cache_miss_after_generation_bump() {
        // A request that built its key at the account's old generation produces
        // a different key at the new generation — a miss.
        let cache = test_cache();
        let hash = U256::from(10u64);
        let cid = U256::from(20u64);
        let wallet = wallet_from_low_u64(0x1234);

        let key = cache.execute_action_key(hash, cache.account_generation(wallet), cid);
        cache.execute_action.insert(key.clone(), true).await;

        cache.bump_account_generation(wallet);

        let new_key = cache.execute_action_key(hash, cache.account_generation(wallet), cid);
        assert_ne!(key, new_key);
        assert_eq!(cache.execute_action.get(&new_key).await, None);
        assert_eq!(cache.execute_action.get(&key).await, Some(true));
    }

    #[tokio::test]
    async fn invalidation_is_scoped_to_one_account() {
        // Bumping account A must not invalidate account B (no global flush).
        let cache = test_cache();
        let hash_a = U256::from(100u64);
        let hash_b = U256::from(200u64);
        let cid = U256::from(50u64);
        let wallet_a = wallet_from_low_u64(0xa);
        let wallet_b = wallet_from_low_u64(0xb);

        let key_a = cache.execute_action_key(hash_a, cache.account_generation(wallet_a), cid);
        let key_b = cache.execute_action_key(hash_b, cache.account_generation(wallet_b), cid);
        cache.execute_action.insert(key_a.clone(), true).await;
        cache.execute_action.insert(key_b.clone(), false).await;

        cache.bump_account_generation(wallet_a);

        // Account A misses (key changed); account B still hits.
        let new_key_a = cache.execute_action_key(hash_a, cache.account_generation(wallet_a), cid);
        let new_key_b = cache.execute_action_key(hash_b, cache.account_generation(wallet_b), cid);
        assert_ne!(key_a, new_key_a);
        assert_eq!(key_b, new_key_b);
        assert_eq!(cache.execute_action.get(&new_key_a).await, None);
        assert_eq!(cache.execute_action.get(&new_key_b).await, Some(false));
    }

    #[tokio::test]
    async fn master_and_usage_keys_share_account_generation() {
        // Two distinct calling-key hashes under the same wallet are both
        // invalidated by a single bump of that wallet's generation.
        let cache = test_cache();
        let master = U256::from(0x1111u64);
        let usage = U256::from(0x2222u64);
        let cid = U256::from(7u64);
        let wallet = wallet_from_low_u64(0xface);

        let g0 = cache.account_generation(wallet);
        let master_key = cache.execute_action_key(master, g0, cid);
        let usage_key = cache.execute_action_key(usage, g0, cid);
        cache.execute_action.insert(master_key.clone(), true).await;
        cache.execute_action.insert(usage_key.clone(), true).await;

        cache.bump_account_generation(wallet);

        let g1 = cache.account_generation(wallet);
        assert_eq!(
            cache
                .execute_action
                .get(&cache.execute_action_key(master, g1, cid))
                .await,
            None
        );
        assert_eq!(
            cache
                .execute_action
                .get(&cache.execute_action_key(usage, g1, cid))
                .await,
            None
        );
    }
}
