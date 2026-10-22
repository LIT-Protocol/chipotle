//! Global blockchain data cache.
//!
//! Caches the results of on-chain permission checks (`canExecuteAction`,
//! `canUseWalletInAction`, `canExecuteActionAndUseWallet`), wallet derivation
//! lookups (`getWalletDerivation`) and calling usage keys' expirations
//! (`listApiKeys`), so that repeated calls for the same API key and relevant
//! parameters avoid redundant contract calls.
//!
//! Permission grants expire after 5 minutes without a cache hit; each read
//! renews that window, but never past the calling usage key's on-chain
//! expiration. The contract compares the expiration with `block.timestamp`
//! and nothing is emitted when a key expires, so no event can invalidate the
//! grant. Each grant therefore carries the key's [`GrantDeadline`]: its TTL is
//! capped at the deadline on insert and on every read, and [`get_or_fetch`]
//! re-checks the deadline on every hit. Denials expire 30 seconds after
//! insertion/update and reads do not extend them. Wallet derivations retain a
//! fixed 5-minute TTL. `try_get_with` coalesces concurrent misses per key.
//!
//! A key's deadline is read with `listApiKeys`, concurrently with the
//! permission check it bounds ([`BlockchainCache::with_deadline`]), and is
//! cached per key generation for 5 minutes, or 30 seconds if the read failed.
//! Account-wide invalidations already list the account's usage keys, so they
//! also seed each listed key's deadline at its new generation: the next
//! uncached grant of each key then costs only its permission check. Keys named
//! by the account events being processed are not seeded (see
//! [`invalidate_for_account_hash`]).
//!
//! API mutation hooks and the account-event listener invalidate both grants
//! and denials using per-account generations embedded in cache keys. A bump
//! makes subsequent lookups miss regardless of recent reads. Each replica
//! must observe events independently; invalidation is process-local.
//!
//! Tradeoff: there is NO hard maximum age for continuously used grants of a
//! key that never expires. A missed event, stopped listener, or incomplete
//! usage-key invalidation can leave a hot stale grant usable indefinitely, or
//! until the key's expiration if it has one. The 30-second denial TTL
//! remains a fallback when a grant event is not successfully invalidated.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alloy::primitives::{Address, U256};
use moka::Expiry;
use moka::future::Cache;

/// Idle expiration for positive permissions and fixed TTL for wallet
/// derivations — 5 minutes. Permission reads renew this window; derivations
/// retain their hard time-to-live cap.
const CACHE_TTL_SECS: u64 = 300;

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

/// Usage keys requested per `listApiKeys` page when resolving a deadline.
const DEADLINE_PAGE_SIZE: u64 = 100;

/// Pages scanned when resolving a deadline: the first 1000 usage keys, the
/// same bound account invalidation uses. Grants to keys beyond it are served
/// but not cached.
const DEADLINE_MAX_PAGES: u64 = 10;

/// Wall-clock time since the Unix epoch, the timebase of on-chain
/// expirations. A clock set before the epoch leaves no time before any
/// deadline.
fn unix_now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::MAX)
}

/// Absolute deadline for serving a cached grant: the calling usage key's
/// on-chain `expiration`.
///
/// Mirrors `ViewsFacet._isExpired`: a key with a non-zero expiration
/// authorizes nothing once `block.timestamp >= expiration`, and `0` never
/// expires. Deadlines are compared with this node's wall clock, which can
/// differ from block timestamps by a few seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantDeadline {
    /// The key never expires, or the result grants nothing.
    Never,
    /// No grant is served at or after this time since the Unix epoch.
    At(Duration),
}

impl GrantDeadline {
    /// The key's expiration could not be resolved: its grants are not cached.
    pub const UNRESOLVED: Self = Self::At(Duration::ZERO);

    /// Deadline for a usage key's on-chain `expiration` in Unix seconds.
    pub fn from_expiration(expiration: U256) -> Self {
        if expiration.is_zero() {
            Self::Never
        } else {
            Self::At(Duration::from_secs(expiration.saturating_to::<u64>()))
        }
    }

    /// Whether a grant may still be served at `now` (time since the epoch).
    pub fn allows(&self, now: Duration) -> bool {
        match self {
            Self::Never => true,
            Self::At(deadline) => now < *deadline,
        }
    }

    /// Cap `ttl` so an entry inserted or read at `now` expires by the deadline.
    fn cap(&self, ttl: Duration, now: Duration) -> Duration {
        match self {
            Self::Never => ttl,
            Self::At(deadline) => ttl.min(deadline.saturating_sub(now)),
        }
    }
}

/// Result of an on-chain permission check stored by the permission caches.
pub trait PermissionResult: Clone + Send + Sync + 'static {
    /// Whether the request may proceed. Such grants get the full TTL and
    /// reads renew it; anything else is cached as a denial.
    fn is_grant(&self) -> bool;

    /// Whether any part of the result authorizes something, so it must not be
    /// served past the key's deadline.
    fn grants_any(&self) -> bool;
}

impl PermissionResult for bool {
    fn is_grant(&self) -> bool {
        *self
    }

    fn grants_any(&self) -> bool {
        *self
    }
}

/// `(can_execute, can_use_wallet)`: the request only proceeds when both are
/// true, but `can_execute` alone is still an execution grant.
impl PermissionResult for (bool, bool) {
    fn is_grant(&self) -> bool {
        self.0 && self.1
    }

    fn grants_any(&self) -> bool {
        self.0 || self.1
    }
}

/// A cached permission result and the deadline after which it is not served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedPermission<T> {
    pub result: T,
    pub deadline: GrantDeadline,
}

impl<T> CachedPermission<T> {
    pub fn new(result: T, deadline: GrantDeadline) -> Self {
        Self { result, deadline }
    }
}

impl<T: PermissionResult> CachedPermission<T> {
    /// TTL from `now`: full for grants, short for denials, capped at the
    /// deadline.
    fn ttl(&self, now: Duration) -> Duration {
        self.deadline
            .cap(permission_ttl(self.result.is_grant()), now)
    }
}

/// Per-entry expiry for the permission caches. Denials expire after
/// `NEGATIVE_CACHE_TTL_SECS` and reads do not extend them. Grants get
/// `CACHE_TTL_SECS` and each read renews it. Every TTL is capped at the
/// entry's deadline, so renewal cannot carry a grant past the key's expiry.
struct PermissionExpiry;

impl<T: PermissionResult> Expiry<String, CachedPermission<T>> for PermissionExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &CachedPermission<T>,
        _created_at: Instant,
    ) -> Option<Duration> {
        Some(value.ttl(unix_now()))
    }

    fn expire_after_update(
        &self,
        _key: &String,
        value: &CachedPermission<T>,
        _updated_at: Instant,
        _duration_until_expiry: Option<Duration>,
    ) -> Option<Duration> {
        Some(value.ttl(unix_now()))
    }

    fn expire_after_read(
        &self,
        _key: &String,
        value: &CachedPermission<T>,
        _read_at: Instant,
        duration_until_expiry: Option<Duration>,
        _last_modified_at: Instant,
    ) -> Option<Duration> {
        if value.result.is_grant() {
            Some(value.ttl(unix_now()))
        } else {
            duration_until_expiry
        }
    }
}

/// Serve a permission result from `cache`, fetching it with `fetch` on a miss.
///
/// [`PermissionExpiry`] already evicts a grant by its deadline. This also
/// re-checks the deadline on every hit, so a wall-clock step since the TTL
/// was computed cannot extend a grant. A stale grant is dropped and fetched
/// again. A result fetched by this call is returned as is: it reflects the
/// chain, whose block timestamps can trail the wall clock.
pub async fn get_or_fetch<T, F, Fut>(
    cache: &Cache<String, CachedPermission<T>>,
    key: String,
    fetch: F,
) -> anyhow::Result<T>
where
    T: PermissionResult,
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<CachedPermission<T>>>,
{
    let mut fetched = false;
    let cached = cache
        .try_get_with(key.clone(), async {
            fetched = true;
            fetch().await
        })
        .await
        .map_err(|e: Arc<anyhow::Error>| anyhow::anyhow!("{:#}", e))?;
    if fetched || cached.deadline.allows(unix_now()) {
        return Ok(cached.result);
    }
    cache.invalidate(&key).await;
    Ok(fetch().await?.result)
}

/// Find `api_key_hash`'s deadline in `listApiKeys` pages of
/// `(apiKeyHash, expiration)`, scanning at most `DEADLINE_MAX_PAGES`. A key
/// that is not listed (an account key, or one beyond the bound) resolves to
/// [`GrantDeadline::UNRESOLVED`].
async fn find_grant_deadline<F, Fut>(
    api_key_hash: U256,
    mut page: F,
) -> anyhow::Result<GrantDeadline>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = anyhow::Result<Vec<(U256, U256)>>>,
{
    for page_number in 0..DEADLINE_MAX_PAGES {
        let keys = page(page_number).await?;
        if let Some((_, expiration)) = keys.iter().find(|(hash, _)| *hash == api_key_hash) {
            return Ok(GrantDeadline::from_expiration(*expiration));
        }
        // `listApiKeys` returns a short page once the list is exhausted.
        if (keys.len() as u64) < DEADLINE_PAGE_SIZE {
            break;
        }
    }
    Ok(GrantDeadline::UNRESOLVED)
}

/// A calling key's deadline lookup, as stored by the deadline cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadlineLookup {
    /// Read from `listApiKeys`: [`GrantDeadline::UNRESOLVED`] if the key is
    /// not listed.
    Resolved(GrantDeadline),
    /// The read failed. The key's grants are not cached until it succeeds.
    Failed,
}

impl DeadlineLookup {
    /// The deadline to cap grants at; a failed lookup allows no caching.
    pub fn deadline(&self) -> GrantDeadline {
        match self {
            Self::Resolved(deadline) => *deadline,
            Self::Failed => GrantDeadline::UNRESOLVED,
        }
    }

    /// How long the lookup is kept from insertion: a failure only briefly,
    /// so a transient RPC error does not leave the key's grants uncached for
    /// the full TTL.
    fn ttl(&self) -> Duration {
        match self {
            Self::Resolved(_) => Duration::from_secs(CACHE_TTL_SECS),
            Self::Failed => Duration::from_secs(NEGATIVE_CACHE_TTL_SECS),
        }
    }
}

/// Per-entry expiry for the deadline cache: [`DeadlineLookup::ttl`] from
/// insertion or update. Reads do not extend it (the trait's default).
struct DeadlineExpiry;

impl Expiry<String, DeadlineLookup> for DeadlineExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &DeadlineLookup,
        _created_at: Instant,
    ) -> Option<Duration> {
        Some(value.ttl())
    }

    fn expire_after_update(
        &self,
        _key: &String,
        value: &DeadlineLookup,
        _updated_at: Instant,
        _duration_until_expiry: Option<Duration>,
    ) -> Option<Duration> {
        Some(value.ttl())
    }
}

/// Deadline-cache key for `api_key_hash` (decimal) at generation `generation`.
fn deadline_key(api_key_hash: &str, generation: u64) -> String {
    format!("{api_key_hash}:g{generation}:dl")
}

/// Maximum entries per cache.
const MAX_CAPACITY: u64 = 100_000;

/// Caches blockchain permission check results with per-account invalidation.
pub struct BlockchainCache {
    /// `can_execute_action` results.
    execute_action: Cache<String, CachedPermission<bool>>,
    /// `can_use_wallet_in_action` results.
    use_wallet: Cache<String, CachedPermission<bool>>,
    /// `can_execute_action_and_use_wallet` results.
    execute_and_wallet: Cache<String, CachedPermission<(bool, bool)>>,
    /// `get_wallet_derivation` results.
    wallet_derivation: Cache<String, U256>,
    /// Calling keys' grant deadlines, from their on-chain expirations.
    grant_deadlines: Cache<String, DeadlineLookup>,
    /// Per-account generation counter keyed by the string representation of
    /// the api_key_hash (`U256`). Uses a plain HashMap (no eviction) to
    /// guarantee that a bumped generation is never lost. Each entry is ~100
    /// bytes; even 100k accounts is only ~10MB.
    generations: RwLock<HashMap<String, u64>>,
    /// Generation bumps of any key so far, counted under the `generations`
    /// write lock. Seeding compares it across a chain read to detect a bump
    /// the read may predate.
    bumps: AtomicU64,
}

impl BlockchainCache {
    fn new() -> Self {
        let ttl = Duration::from_secs(CACHE_TTL_SECS);
        // Permission expiry renews grants on reads, never past their deadline,
        // and leaves denials fixed. Do not add a hard TTL: it would cap the
        // sliding window of keys that never expire.
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
            .expire_after(PermissionExpiry)
            .build();
        let wallet_derivation = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .time_to_idle(ttl)
            .time_to_live(ttl)
            .build();
        let grant_deadlines = Cache::builder()
            .max_capacity(MAX_CAPACITY)
            .expire_after(DeadlineExpiry)
            .build();
        Self {
            execute_action,
            use_wallet,
            execute_and_wallet,
            wallet_derivation,
            grant_deadlines,
            generations: RwLock::new(HashMap::new()),
            bumps: AtomicU64::new(0),
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

    /// Build a cache key for a calling key's grant deadline. The generation
    /// makes usage-key mutations (which can change the expiration) re-read it.
    pub fn grant_deadline_key(&self, api_key_hash: U256) -> String {
        let h = api_key_hash.to_string();
        let g = self.generation(&h);
        deadline_key(&h, g)
    }

    /// Reference to the `can_execute_action` cache.
    pub fn execute_action_cache(&self) -> &Cache<String, CachedPermission<bool>> {
        &self.execute_action
    }

    /// Reference to the `can_use_wallet_in_action` cache.
    pub fn use_wallet_cache(&self) -> &Cache<String, CachedPermission<bool>> {
        &self.use_wallet
    }

    /// Reference to the `can_execute_action_and_use_wallet` cache.
    pub fn execute_and_wallet_cache(&self) -> &Cache<String, CachedPermission<(bool, bool)>> {
        &self.execute_and_wallet
    }

    /// Reference to the `get_wallet_derivation` cache.
    pub fn wallet_derivation_cache(&self) -> &Cache<String, U256> {
        &self.wallet_derivation
    }

    /// Reference to the grant-deadline cache.
    pub fn grant_deadline_cache(&self) -> &Cache<String, DeadlineLookup> {
        &self.grant_deadlines
    }

    /// Run the on-chain permission `check` for `api_key_hash` and pair its
    /// result with the deadline that bounds caching it.
    ///
    /// The deadline is resolved concurrently with the check, so an uncached
    /// grant waits for one chain round trip rather than two. Denials need no
    /// deadline and get [`GrantDeadline::Never`], but they still wait for the
    /// lookup; it is cached per key, so a denied key costs at most one extra
    /// lookup per deadline TTL. The lookup also completes when the check
    /// fails: dropping moka's `get_with` mid-lookup makes the callers waiting
    /// on it retry, and it panics them after 200 such retries.
    pub async fn with_deadline<T, F>(
        &self,
        api_key_hash: U256,
        check: F,
    ) -> anyhow::Result<CachedPermission<T>>
    where
        T: PermissionResult,
        F: Future<Output = anyhow::Result<T>>,
    {
        let (result, deadline) = tokio::join!(check, self.grant_deadline(api_key_hash));
        let result = result?;
        let deadline = if result.grants_any() {
            deadline
        } else {
            GrantDeadline::Never
        };
        Ok(CachedPermission::new(result, deadline))
    }

    /// Resolve `api_key_hash`'s on-chain expiration via `listApiKeys`, cached
    /// per key generation. A failed lookup resolves to
    /// [`GrantDeadline::UNRESOLVED`] for `NEGATIVE_CACHE_TTL_SECS`: grants are
    /// still served from the chain but not cached, and the key is not
    /// re-scanned on every request.
    async fn grant_deadline(&self, api_key_hash: U256) -> GrantDeadline {
        let key = self.grant_deadline_key(api_key_hash);
        self.grant_deadlines
            .get_with(key, async move {
                let page = |page_number: u64| async move {
                    let keys = super::list_api_keys_by_hash(
                        api_key_hash,
                        U256::from(page_number),
                        U256::from(DEADLINE_PAGE_SIZE),
                    )
                    .await?;
                    Ok(keys
                        .into_iter()
                        .map(|key| (key.apiKeyHash, key.expiration))
                        .collect())
                };
                match find_grant_deadline(api_key_hash, page).await {
                    Ok(deadline) => DeadlineLookup::Resolved(deadline),
                    Err(e) => {
                        tracing::warn!(
                            "blockchain_cache: failed to resolve the expiration of {api_key_hash}: \
                             {e:#}. Its grants will not be cached; retrying in \
                             {NEGATIVE_CACHE_TTL_SECS}s."
                        );
                        DeadlineLookup::Failed
                    }
                }
            })
            .await
            .deadline()
    }

    /// Number of tracked per-account generation counters (grows monotonically;
    /// entries are never evicted). Exposed via `/get_system_stats`.
    pub fn generation_count(&self) -> u64 {
        self.generations
            .read()
            .expect("generation lock poisoned")
            .len() as u64
    }

    /// Bump the generation for a single api_key_hash, invalidating all cached
    /// permission entries for that key.
    fn bump_generation(&self, api_key_hash: &str) {
        let mut generations = self.generations.write().expect("generation lock poisoned");
        self.bump_locked(&mut generations, api_key_hash);
    }

    /// Bump `api_key_hash` in the write-locked `generations`; returns the new
    /// generation.
    fn bump_locked(&self, generations: &mut HashMap<String, u64>, api_key_hash: &str) -> u64 {
        let entry = generations.entry(api_key_hash.to_string()).or_insert(0);
        *entry = entry.wrapping_add(1);
        let next = *entry;
        self.bumps.fetch_add(1, Ordering::SeqCst);
        tracing::debug!(
            "blockchain_cache: bumped generation for {} to {}",
            api_key_hash,
            next
        );
        next
    }

    /// Generation bumps of any key so far. Read it before a chain read whose
    /// result [`Self::bump_and_seed_deadlines`] will seed.
    fn bump_count(&self) -> u64 {
        self.bumps.load(Ordering::SeqCst)
    }

    /// Bump every usage key in `usage_keys`, `(apiKeyHash, expiration)` pairs
    /// from one `listApiKeys` read, except `already_bumped`. Then seed each
    /// listed key's grant deadline at its current generation from that read,
    /// so its next uncached grant needs no `listApiKeys` scan of its own. Keys
    /// in `unseeded` are bumped but not seeded.
    ///
    /// `bumps_before` is [`Self::bump_count`] from before the read was sent.
    /// The read predates these bumps, so nothing is seeded if any generation
    /// was bumped since: that bump may stand for a change the read missed,
    /// such as an API hook's usage-key update mined during the read. Keys
    /// bumped after the seeding move to a new generation, which the seed does
    /// not reach.
    async fn bump_and_seed_deadlines(
        &self,
        usage_keys: &[(U256, U256)],
        already_bumped: Option<U256>,
        unseeded: &HashSet<U256>,
        bumps_before: u64,
    ) {
        let seeds: Vec<(String, GrantDeadline)> = {
            let mut generations = self.generations.write().expect("generation lock poisoned");
            let undisturbed = self.bump_count() == bumps_before;
            let mut seeds = Vec::new();
            for &(hash, expiration) in usage_keys {
                let h = hash.to_string();
                let g = if Some(hash) == already_bumped {
                    generations.get(&h).copied().unwrap_or(0)
                } else {
                    self.bump_locked(&mut generations, &h)
                };
                if undisturbed && !unseeded.contains(&hash) {
                    seeds.push((
                        deadline_key(&h, g),
                        GrantDeadline::from_expiration(expiration),
                    ));
                }
            }
            seeds
        };
        for (key, deadline) in seeds {
            self.grant_deadlines
                .insert(key, DeadlineLookup::Resolved(deadline))
                .await;
        }
    }
}

static BLOCKCHAIN_CACHE_INSTANCE: OnceLock<BlockchainCache> = OnceLock::new();

/// Initialize the global blockchain cache. Call once during startup.
pub fn init() {
    BLOCKCHAIN_CACHE_INSTANCE.get_or_init(BlockchainCache::new);
    tracing::info!(
        "blockchain_cache: initialized (grant idle expiry={CACHE_TTL_SECS}s, negative TTL={NEGATIVE_CACHE_TTL_SECS}s)"
    );
}

/// Get the global cache instance. Returns `None` if not initialized.
pub fn get() -> Option<&'static BlockchainCache> {
    BLOCKCHAIN_CACHE_INSTANCE.get()
}

/// Invalidate cached permission entries for the given `api_key_hash` (`U256`).
///
/// Unlike [`invalidate_for_key`], this takes the already-computed hash directly,
/// matching the generation-map key format used by the `*_key` builders. It is
/// the entry point used by the on-chain account-event listener
/// ([`crate::account_events`]), which receives `apiKeyHash` values straight from
/// emitted contract events.
pub fn invalidate_for_hash(api_key_hash: U256) {
    if let Some(cache) = get() {
        cache.bump_generation(&api_key_hash.to_string());
    }
}

/// Invalidate cached permission entries for an entire account identified by its
/// on-chain `apiKeyHash`: the account (master) key and every usage key under it.
///
/// Hash-based analogue of [`invalidate_for_account`], used by the on-chain
/// account-event listener ([`crate::account_events`]). Account-level mutation
/// events (group/action/PKP) carry only the master `apiKeyHash`, but cached
/// permission entries are keyed per *calling* key (master or usage), so
/// usage-key-authenticated traffic would otherwise serve stale results.
/// Resolves the usage keys via a chain call to `list_api_keys_by_hash`, and
/// seeds each listed key's grant deadline from the same call.
///
/// Keys in `changed`, those named by the events being processed, are bumped
/// but not seeded. The listing is read right after the events' block was
/// observed, possibly from an RPC node that has not imported it yet, so it can
/// predate exactly the change those events made, such as a `UsageApiKeySet`
/// that moves a key's expiration earlier. Such a key reads its own expiration
/// at its next uncached grant, concurrently with the permission check.
///
/// **Limitation:** Only the first 1000 usage keys are invalidated, matching
/// [`invalidate_for_account`]. Grants to keys beyond that are never cached:
/// their expiration lookup scans the same first 1000 keys and fails closed.
/// Their denials and wallet derivations still rely on expiration.
pub async fn invalidate_for_account_hash(account_api_key_hash: U256, changed: &HashSet<U256>) {
    let Some(cache) = get() else { return };
    cache.bump_generation(&account_api_key_hash.to_string());
    let bumps_before = cache.bump_count();

    match super::list_api_keys_by_hash(account_api_key_hash, U256::ZERO, U256::from(1000u64)).await
    {
        Ok(usage_keys) => {
            let listed: Vec<(U256, U256)> = usage_keys
                .iter()
                .map(|uk| (uk.apiKeyHash, uk.expiration))
                .collect();
            cache
                .bump_and_seed_deadlines(&listed, None, changed, bumps_before)
                .await;
            tracing::debug!(
                "blockchain_cache: invalidated account {} ({} usage keys)",
                account_api_key_hash,
                usage_keys.len()
            );
        }
        Err(e) => {
            tracing::warn!(
                "blockchain_cache: failed to list usage keys for account {account_api_key_hash}: {e}. \
                 Usage-key cache entries may be stale; hot grants can persist indefinitely."
            );
        }
    }
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
/// Fetches usage key hashes via a chain call to `list_api_keys`, and seeds
/// each listed key's grant deadline from the same call. Call after
/// group/action/PKP mutations where any key under the account could be affected.
/// Those do not change usage keys' expirations (usage-key mutations call
/// [`invalidate_for_keys`]), so every listed key is seeded.
///
/// **Limitation:** If the caller authenticates with a usage key (not the master
/// key), the master key's cached entries are NOT invalidated here because the
/// contract does not expose a `resolveToMaster` view. The event listener can
/// invalidate the master independently; without that, hot master grants can
/// remain stale indefinitely.
pub async fn invalidate_for_account(api_key: &str) {
    let Some(cache) = get() else { return };

    // Always bump the calling key (master or usage).
    let caller_hash = crate::utils::parse_with_hash::api_key_hash(api_key);
    cache.bump_generation(&caller_hash.to_string());
    let bumps_before = cache.bump_count();

    // Fetch all usage keys under this account and bump each one.
    // list_api_keys resolves both master and usage keys to the correct account.
    match super::list_api_keys(api_key, U256::ZERO, U256::from(1000u64)).await {
        Ok(usage_keys) => {
            let listed: Vec<(U256, U256)> = usage_keys
                .iter()
                .map(|uk| (uk.apiKeyHash, uk.expiration))
                .collect();
            cache
                .bump_and_seed_deadlines(&listed, Some(caller_hash), &HashSet::new(), bumps_before)
                .await;
            tracing::debug!(
                "blockchain_cache: invalidated account ({} usage keys)",
                usage_keys.len()
            );
        }
        Err(e) => {
            tracing::warn!(
                "blockchain_cache: failed to list usage keys for invalidation: {e}. \
                 Usage key cache entries may be stale; hot grants can persist indefinitely."
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
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_cache() -> BlockchainCache {
        BlockchainCache::new()
    }

    fn addr_from_low_u64(n: u64) -> Address {
        let mut bytes = [0u8; 20];
        bytes[12..].copy_from_slice(&n.to_be_bytes());
        Address::from(bytes)
    }

    /// A result for a key that never expires.
    fn cached<T>(result: T) -> CachedPermission<T> {
        CachedPermission::new(result, GrantDeadline::Never)
    }

    /// A result for a key that expires at `deadline` (time since the epoch).
    fn expiring<T>(result: T, deadline: Duration) -> CachedPermission<T> {
        CachedPermission::new(result, GrantDeadline::At(deadline))
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

    #[test]
    fn grant_deadline_key_follows_generation() {
        let cache = test_cache();
        let hash = U256::from(42u64);
        assert_eq!(cache.grant_deadline_key(hash), "42:g0:dl");
        cache.bump_generation(&hash.to_string());
        assert_eq!(cache.grant_deadline_key(hash), "42:g1:dl");
    }

    // ── Cache hit / miss with generation ────────────────────────────

    #[tokio::test]
    async fn cache_hit_returns_stored_value() {
        let cache = test_cache();
        let hash = U256::from(10u64);
        let cid = U256::from(20u64);

        let key = cache.execute_action_key(hash, cid);
        cache.execute_action.insert(key.clone(), cached(true)).await;

        // Same key should hit
        let key2 = cache.execute_action_key(hash, cid);
        assert_eq!(key, key2);
        assert_eq!(cache.execute_action.get(&key2).await, Some(cached(true)));
    }

    #[tokio::test]
    async fn cache_miss_after_invalidation() {
        let cache = test_cache();
        let hash = U256::from(10u64);
        let cid = U256::from(20u64);

        // Populate cache
        let key = cache.execute_action_key(hash, cid);
        cache.execute_action.insert(key.clone(), cached(true)).await;

        // Bump generation
        cache.bump_generation(&hash.to_string());

        // New key should be different — cache miss
        let new_key = cache.execute_action_key(hash, cid);
        assert_ne!(key, new_key);
        assert_eq!(cache.execute_action.get(&new_key).await, None);

        // Old key entry still exists (evicted by TTL later)
        assert_eq!(cache.execute_action.get(&key).await, Some(cached(true)));
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
        cache
            .execute_action
            .insert(key_a.clone(), cached(true))
            .await;
        cache
            .execute_action
            .insert(key_b.clone(), cached(false))
            .await;

        // Invalidate only account A
        cache.bump_generation(&hash_a.to_string());

        // Account A key changed (miss)
        let new_key_a = cache.execute_action_key(hash_a, cid);
        assert_ne!(key_a, new_key_a);
        assert_eq!(cache.execute_action.get(&new_key_a).await, None);

        // Account B key unchanged (still hits)
        let new_key_b = cache.execute_action_key(hash_b, cid);
        assert_eq!(key_b, new_key_b);
        assert_eq!(
            cache.execute_action.get(&new_key_b).await,
            Some(cached(false))
        );
    }

    // ── use_wallet and execute_and_wallet caches ────────────────────

    #[tokio::test]
    async fn use_wallet_cache_hit_and_invalidation() {
        let cache = test_cache();
        let hash = U256::from(5u64);
        let cid = U256::from(6u64);
        let wallet = addr_from_low_u64(0xbeef);

        let key = cache.use_wallet_key(hash, cid, wallet);
        cache.use_wallet.insert(key.clone(), cached(true)).await;
        assert_eq!(cache.use_wallet.get(&key).await, Some(cached(true)));

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
            .insert(key.clone(), cached((true, false)))
            .await;
        assert_eq!(
            cache.execute_and_wallet.get(&key).await,
            Some(cached((true, false)))
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
            .try_get_with(key.clone(), async { Ok::<_, anyhow::Error>(cached(true)) })
            .await
            .unwrap();
        assert!(result.result);

        // Second call should hit the cache (no closure needed)
        let result2 = cache
            .execute_action
            .try_get_with(key, async {
                Err::<CachedPermission<bool>, anyhow::Error>(anyhow::anyhow!(
                    "should not be called on cache hit"
                ))
            })
            .await
            .expect("should have been a cache hit");
        assert!(result2.result);
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
            .try_get_with(key, async { Ok::<_, anyhow::Error>(cached(true)) })
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
                Ok::<_, anyhow::Error>(cached(false)) // different value proves it re-fetched
            })
            .await
            .unwrap();
        assert!(
            closure_called,
            "closure should run on cache miss after bump"
        );
        assert!(!result.result, "should return the newly fetched value");
    }

    #[test]
    fn permission_reads_extend_only_grants() {
        let now = Instant::now();
        let key = "permission".to_string();
        let remaining = Some(Duration::from_secs(7));
        assert_eq!(
            PermissionExpiry.expire_after_read(&key, &cached(true), now, remaining, now),
            Some(Duration::from_secs(CACHE_TTL_SECS))
        );
        assert_eq!(
            PermissionExpiry.expire_after_read(&key, &cached(false), now, remaining, now),
            remaining
        );
    }

    #[test]
    fn pair_permission_reads_extend_only_complete_grants() {
        let now = Instant::now();
        let key = "permission-pair".to_string();
        let remaining = Some(Duration::from_secs(7));
        for value in [(true, true), (true, false), (false, true), (false, false)] {
            assert_eq!(
                PermissionExpiry.expire_after_read(&key, &cached(value), now, remaining, now),
                if value.0 && value.1 {
                    Some(Duration::from_secs(CACHE_TTL_SECS))
                } else {
                    remaining
                }
            );
        }
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
            PermissionExpiry.expire_after_create(&key, &cached(true), now),
            Some(Duration::from_secs(CACHE_TTL_SECS))
        );
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &cached(false), now),
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
                &cached(true),
                now,
                Some(Duration::from_secs(NEGATIVE_CACHE_TTL_SECS))
            ),
            Some(Duration::from_secs(CACHE_TTL_SECS))
        );
        assert_eq!(
            PermissionExpiry.expire_after_update(
                &key,
                &cached(false),
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
            PermissionExpiry.expire_after_create(&key, &cached((true, true)), now),
            full
        );
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &cached((true, false)), now),
            short
        );
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &cached((false, true)), now),
            short
        );
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &cached((false, false)), now),
            short
        );
        assert_eq!(
            PermissionExpiry.expire_after_update(&key, &cached((true, true)), now, short),
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

    // ── Grant deadlines (usage-key expiration) ───────────────────────

    #[test]
    fn deadline_mirrors_contract_expiration() {
        assert_eq!(
            GrantDeadline::from_expiration(U256::ZERO),
            GrantDeadline::Never
        );
        assert_eq!(
            GrantDeadline::from_expiration(U256::MAX),
            GrantDeadline::At(Duration::from_secs(u64::MAX))
        );
        // `_isExpired`: a key is expired once `block.timestamp >= expiration`.
        let deadline = GrantDeadline::from_expiration(U256::from(1_000u64));
        assert!(deadline.allows(Duration::from_millis(999_999)));
        assert!(!deadline.allows(Duration::from_secs(1_000)));
        assert!(!deadline.allows(Duration::from_secs(1_001)));
        assert!(GrantDeadline::Never.allows(Duration::MAX));
        assert!(!GrantDeadline::UNRESOLVED.allows(Duration::ZERO));
    }

    #[test]
    fn grant_ttl_is_capped_at_deadline() {
        let secs = Duration::from_secs;
        let now = secs(1_000_000);
        let full = secs(CACHE_TTL_SECS);
        let short = secs(NEGATIVE_CACHE_TTL_SECS);
        assert_eq!(cached(true).ttl(now), full);
        assert_eq!(expiring(true, now + secs(10)).ttl(now), secs(10));
        assert_eq!(expiring(true, now + secs(10_000)).ttl(now), full);
        // At or past the deadline, or unresolved: not cached at all.
        assert_eq!(expiring(true, now).ttl(now), Duration::ZERO);
        assert_eq!(expiring(true, now - secs(1)).ttl(now), Duration::ZERO);
        assert_eq!(
            CachedPermission::new(true, GrantDeadline::UNRESOLVED).ttl(now),
            Duration::ZERO
        );
        // Pairs: complete grants get the full TTL and partial grants the
        // short one; both stop at the deadline.
        assert_eq!(expiring((true, true), now + secs(10)).ttl(now), secs(10));
        assert_eq!(expiring((true, false), now + secs(10)).ttl(now), secs(10));
        assert_eq!(expiring((true, false), now + secs(10_000)).ttl(now), short);
        assert_eq!(cached(false).ttl(now), short);
    }

    #[test]
    fn expiry_hooks_never_extend_past_deadline() {
        let at = Instant::now();
        let key = "k".to_string();
        let remaining = Some(Duration::from_secs(7));
        let soon = unix_now() + Duration::from_secs(60);
        for ttl in [
            PermissionExpiry.expire_after_create(&key, &expiring(true, soon), at),
            PermissionExpiry.expire_after_update(&key, &expiring(true, soon), at, remaining),
            PermissionExpiry.expire_after_read(&key, &expiring(true, soon), at, remaining, at),
            PermissionExpiry.expire_after_read(
                &key,
                &expiring((true, true), soon),
                at,
                remaining,
                at,
            ),
        ] {
            let ttl = ttl.unwrap();
            assert!(
                ttl <= Duration::from_secs(60) && ttl > Duration::from_secs(50),
                "{ttl:?}"
            );
        }
        let past = unix_now() - Duration::from_secs(1);
        let zero = Some(Duration::ZERO);
        assert_eq!(
            PermissionExpiry.expire_after_create(&key, &expiring(true, past), at),
            zero
        );
        assert_eq!(
            PermissionExpiry.expire_after_update(&key, &expiring(true, past), at, remaining),
            zero
        );
        assert_eq!(
            PermissionExpiry.expire_after_read(&key, &expiring(true, past), at, remaining, at),
            zero
        );
    }

    #[tokio::test]
    async fn sustained_reads_cannot_keep_grant_past_deadline() {
        // A grant read continuously across its key's expiration: reads keep
        // renewing a never-expiring grant but stop at an expiring one's deadline.
        let cache = test_cache();
        let deadline = unix_now() + Duration::from_millis(300);
        let cid = U256::from(2u64);
        let expiring_key = cache.execute_action_key(U256::from(1u64), cid);
        let never_key = cache.execute_action_key(U256::from(3u64), cid);
        cache
            .execute_action
            .insert(expiring_key.clone(), expiring(true, deadline))
            .await;
        cache
            .execute_action
            .insert(never_key.clone(), cached(true))
            .await;
        let (mut hits_before, mut reads_after) = (0, 0);
        while unix_now() < deadline + Duration::from_millis(200) {
            let started = unix_now();
            let hit = cache.execute_action.get(&expiring_key).await;
            if started >= deadline {
                assert_eq!(
                    hit,
                    None,
                    "grant served {:?} past its deadline",
                    started - deadline
                );
                reads_after += 1;
            } else if hit.is_some() {
                hits_before += 1;
            }
            assert_eq!(
                cache.execute_action.get(&never_key).await,
                Some(cached(true))
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            hits_before > 0 && reads_after > 0,
            "{hits_before} hits before, {reads_after} reads after the deadline"
        );
    }

    #[tokio::test]
    async fn get_or_fetch_refetches_grant_found_past_deadline() {
        // No per-entry expiry, standing in for a wall-clock step since the TTL
        // was computed: the stale grant is still stored when it is hit.
        let cache: Cache<String, CachedPermission<bool>> = Cache::builder().build();
        let key = "stale".to_string();
        cache
            .insert(
                key.clone(),
                expiring(true, unix_now() - Duration::from_secs(1)),
            )
            .await;
        let fetches = &AtomicUsize::new(0);
        let result = get_or_fetch(&cache, key.clone(), || async move {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok(cached(false))
        })
        .await
        .unwrap();
        assert!(!result, "a grant past its deadline must be fetched again");
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get(&key).await, None, "the stale grant is dropped");
    }

    #[tokio::test]
    async fn get_or_fetch_serves_live_and_freshly_fetched_grants() {
        let cache = test_cache();
        let live = cache.execute_action_key(U256::from(1u64), U256::from(2u64));
        cache
            .execute_action
            .insert(
                live.clone(),
                expiring(true, unix_now() + Duration::from_secs(60)),
            )
            .await;
        let served = get_or_fetch(&cache.execute_action, live, || async {
            Err::<CachedPermission<bool>, _>(anyhow::anyhow!("a live grant is not re-fetched"))
        })
        .await
        .unwrap();
        assert!(served);

        // The chain is authoritative and its block timestamps can trail the
        // wall clock: a grant fetched by this lookup is served once even if
        // its deadline has passed, but it is not cached.
        let fresh = cache.execute_action_key(U256::from(3u64), U256::from(2u64));
        let fetches = &AtomicUsize::new(0);
        let served = get_or_fetch(&cache.execute_action, fresh.clone(), || async move {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok(CachedPermission::new(true, GrantDeadline::UNRESOLVED))
        })
        .await
        .unwrap();
        assert!(served);
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        assert_eq!(cache.execute_action.get(&fresh).await, None);
    }

    /// `ViewsFacet.listApiKeys` paging over `(apiKeyHash, expiration)`,
    /// including its reset to the first page when `pageSize` exceeds the
    /// list length.
    fn list_page(keys: &[(U256, U256)], page_number: u64, page_size: u64) -> Vec<(U256, U256)> {
        let total = keys.len() as u64;
        let (page_number, page_size) = if page_size > total {
            (0, total)
        } else {
            (page_number, page_size)
        };
        let start = page_number * page_size;
        if start >= total {
            return Vec::new();
        }
        let end = (start + page_size).min(total);
        keys[start as usize..end as usize].to_vec()
    }

    fn usage_keys(count: u64) -> Vec<(U256, U256)> {
        (1..=count)
            .map(|i| (U256::from(i), U256::from(10_000 + i)))
            .collect()
    }

    /// Resolve `target`'s deadline from `keys`; also returns the pages read.
    async fn resolve(keys: &[(U256, U256)], target: U256) -> (GrantDeadline, Vec<u64>) {
        let requested = Mutex::new(Vec::new());
        let deadline = find_grant_deadline(target, |page_number| {
            requested.lock().unwrap().push(page_number);
            let page = list_page(keys, page_number, DEADLINE_PAGE_SIZE);
            async move { Ok::<_, anyhow::Error>(page) }
        })
        .await
        .unwrap();
        (deadline, requested.into_inner().unwrap())
    }

    #[tokio::test]
    async fn deadline_lookup_scans_pages_until_key_found() {
        let keys = usage_keys(250);
        let (deadline, pages) = resolve(&keys, U256::from(231u64)).await;
        assert_eq!(deadline, GrantDeadline::At(Duration::from_secs(10_231)));
        assert_eq!(pages, [0, 1, 2]);
        let (deadline, pages) = resolve(&keys, U256::from(3u64)).await;
        assert_eq!(deadline, GrantDeadline::At(Duration::from_secs(10_003)));
        assert_eq!(pages, [0]);
    }

    #[tokio::test]
    async fn deadline_lookup_maps_zero_expiration_to_never() {
        let mut keys = usage_keys(3);
        keys[1].1 = U256::ZERO;
        assert_eq!(resolve(&keys, keys[1].0).await.0, GrantDeadline::Never);
    }

    #[tokio::test]
    async fn unlisted_key_deadline_is_unresolved() {
        // An account (master) key is not in its own usage-key list.
        let (deadline, pages) = resolve(&usage_keys(30), U256::from(999u64)).await;
        assert_eq!(deadline, GrantDeadline::UNRESOLVED);
        assert_eq!(pages, [0], "a short page ends the scan");
        let (deadline, pages) = resolve(&usage_keys(200), U256::from(999u64)).await;
        assert_eq!(deadline, GrantDeadline::UNRESOLVED);
        assert_eq!(
            pages,
            [0, 1, 2],
            "a full last page is followed by an empty one"
        );
    }

    #[tokio::test]
    async fn deadline_lookup_is_bounded() {
        let keys = usage_keys(DEADLINE_PAGE_SIZE * DEADLINE_MAX_PAGES + 1);
        let (deadline, pages) = resolve(&keys, keys.last().unwrap().0).await;
        assert_eq!(deadline, GrantDeadline::UNRESOLVED);
        assert_eq!(pages, (0..DEADLINE_MAX_PAGES).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn deadline_lookup_propagates_errors() {
        let result = find_grant_deadline(U256::from(1u64), |_| async {
            Err::<Vec<(U256, U256)>, _>(anyhow::anyhow!("rpc unavailable"))
        })
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn denials_ignore_deadline_and_unresolvable_grants_fail_closed() {
        let cache = test_cache();
        let hash = U256::from(165u64);
        let deadline_key = cache.grant_deadline_key(hash);
        let check = |result| async move { anyhow::Ok(result) };
        // No chain client in unit tests, so the expiration lookup fails.
        // Denials are not capped by it.
        assert_eq!(
            cache.with_deadline(hash, check(false)).await.unwrap(),
            cached(false)
        );
        assert_eq!(
            cache
                .with_deadline(hash, async { anyhow::Ok((false, false)) })
                .await
                .unwrap(),
            cached((false, false))
        );

        // Grants are not cacheable, and the failure is cached briefly rather
        // than retried on every request.
        let unresolved = GrantDeadline::UNRESOLVED;
        assert_eq!(
            cache.with_deadline(hash, check(true)).await.unwrap(),
            CachedPermission::new(true, unresolved)
        );
        assert_eq!(
            cache
                .with_deadline(hash, async { anyhow::Ok((true, false)) })
                .await
                .unwrap(),
            CachedPermission::new((true, false), unresolved)
        );
        assert_eq!(
            cache.grant_deadlines.get(&deadline_key).await,
            Some(DeadlineLookup::Failed)
        );

        // A failed check is returned as is, and the lookup still completes
        // rather than being dropped mid-initialization.
        let other = U256::from(166u64);
        let failed = cache
            .with_deadline(other, async { Err::<bool, _>(anyhow::anyhow!("rpc down")) })
            .await;
        assert_eq!(failed.unwrap_err().to_string(), "rpc down");
        assert_eq!(
            cache
                .grant_deadlines
                .get(&cache.grant_deadline_key(other))
                .await,
            Some(DeadlineLookup::Failed)
        );
    }

    #[tokio::test]
    async fn deadline_is_resolved_concurrently_with_the_check() {
        // The check only completes once the expiration lookup (which fails
        // at once without a chain client) has been cached, so a lookup that
        // waited for the check would leave it to give up.
        let cache = test_cache();
        let hash = U256::from(7u64);
        let deadline_key = cache.grant_deadline_key(hash);
        let check = async {
            for _ in 0..1_000 {
                if cache.grant_deadlines.get(&deadline_key).await.is_some() {
                    return anyhow::Ok(true);
                }
                tokio::task::yield_now().await;
            }
            anyhow::bail!("the expiration lookup waited for the permission check")
        };
        assert_eq!(
            cache.with_deadline(hash, check).await.unwrap(),
            CachedPermission::new(true, GrantDeadline::UNRESOLVED)
        );
    }

    #[test]
    fn failed_deadline_lookups_are_kept_only_briefly() {
        let at = Instant::now();
        let key = "k".to_string();
        let full = Some(Duration::from_secs(CACHE_TTL_SECS));
        let short = Some(Duration::from_secs(NEGATIVE_CACHE_TTL_SECS));
        let failed = DeadlineLookup::Failed;
        // Not being listed is a result, not a failure.
        for resolved in [
            GrantDeadline::Never,
            GrantDeadline::At(Duration::from_secs(1)),
            GrantDeadline::UNRESOLVED,
        ]
        .map(DeadlineLookup::Resolved)
        {
            assert_eq!(
                DeadlineExpiry.expire_after_create(&key, &resolved, at),
                full
            );
            assert_eq!(
                DeadlineExpiry.expire_after_update(&key, &resolved, at, short),
                full,
                "a successful lookup replacing a failure gets the full TTL"
            );
            assert_eq!(
                DeadlineExpiry.expire_after_read(&key, &resolved, at, short, at),
                short,
                "reads do not extend a deadline"
            );
        }
        assert_eq!(DeadlineExpiry.expire_after_create(&key, &failed, at), short);
        assert_eq!(
            DeadlineExpiry.expire_after_update(&key, &failed, at, full),
            short
        );
        assert_eq!(failed.deadline(), GrantDeadline::UNRESOLVED);
    }

    // ── Deadline seeding by account invalidation ────────────────────

    async fn seeded(cache: &BlockchainCache, hash: U256) -> Option<DeadlineLookup> {
        cache
            .grant_deadlines
            .get(&cache.grant_deadline_key(hash))
            .await
    }

    #[tokio::test]
    async fn account_invalidation_seeds_listed_deadlines_at_new_generations() {
        let cache = test_cache();
        let (expiring, never, caller, changed) = (
            U256::from(1u64),
            U256::from(2u64),
            U256::from(3u64),
            U256::from(4u64),
        );
        cache.bump_generation(&expiring.to_string());
        // An API hook bumps its caller before reading the account's keys.
        cache.bump_generation(&caller.to_string());
        let stale = cache.grant_deadline_key(expiring);
        cache
            .grant_deadlines
            .insert(
                stale.clone(),
                DeadlineLookup::Resolved(GrantDeadline::Never),
            )
            .await;

        let before = cache.bump_count();
        let listed = [
            (expiring, U256::from(5_000u64)),
            (never, U256::ZERO),
            (caller, U256::from(6_000u64)),
            (changed, U256::from(7_000u64)),
        ];
        cache
            .bump_and_seed_deadlines(&listed, Some(caller), &HashSet::from([changed]), before)
            .await;

        assert_eq!(cache.grant_deadline_key(expiring), "1:g2:dl");
        assert_eq!(cache.grant_deadline_key(never), "2:g1:dl");
        assert_eq!(cache.grant_deadline_key(caller), "3:g1:dl", "bumped once");
        assert_eq!(cache.grant_deadline_key(changed), "4:g1:dl");
        let at = |secs| DeadlineLookup::Resolved(GrantDeadline::At(Duration::from_secs(secs)));
        assert_eq!(seeded(&cache, expiring).await, Some(at(5_000)));
        assert_eq!(
            seeded(&cache, never).await,
            Some(DeadlineLookup::Resolved(GrantDeadline::Never))
        );
        assert_eq!(seeded(&cache, caller).await, Some(at(6_000)));
        assert_eq!(seeded(&cache, changed).await, None, "bumped, not seeded");
        assert_eq!(
            cache.grant_deadlines.get(&stale).await,
            Some(DeadlineLookup::Resolved(GrantDeadline::Never)),
            "the previous generation's entry is left to expire, unreachable"
        );
    }

    #[tokio::test]
    async fn bump_during_the_read_invalidates_without_seeding() {
        let cache = test_cache();
        let (usage, other) = (U256::from(1u64), U256::from(2u64));
        let before = cache.bump_count();
        // Any bump between the read and the seeding, e.g. an API hook after
        // its usage-key update was mined, may stand for a change the read
        // missed.
        cache.bump_generation(&other.to_string());
        cache
            .bump_and_seed_deadlines(&[(usage, U256::ZERO)], None, &HashSet::new(), before)
            .await;
        assert_eq!(cache.generation(&usage.to_string()), 1, "still invalidated");
        assert_eq!(seeded(&cache, usage).await, None);
        // The bump count includes the invalidation's own bumps.
        assert_eq!(cache.bump_count(), before + 2);
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
