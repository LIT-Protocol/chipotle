//! Process-wide, in-memory V8 code cache for compiled user scripts.
//!
//! When a Lit Action's bundled JS lands on the hot path, V8 otherwise
//! re-parses and re-compiles it on every execution — even though the source
//! is identical across executions once it's in the `ActionCodeCache`.
//!
//! `deno_runtime` exposes a `CodeCache` trait that lets us hand V8 the
//! already-compiled bytecode on subsequent runs. We keep it in memory with
//! the same 100MB bound as the action-code cache, so the two caches together
//! give "full caching of JavaScript" for cached actions (CPL-264).

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::sync::{Arc, RwLock};

use deno_core::ModuleSpecifier;
use deno_runtime::code_cache::{CodeCache, CodeCacheType};

const MAX_V8_CODE_CACHE_BYTES: usize = 100 * 1024 * 1024;

pub type SharedV8CodeCache = Arc<V8CodeCache>;

pub fn new_v8_code_cache() -> SharedV8CodeCache {
    Arc::new(V8CodeCache::default())
}

#[derive(Default)]
pub struct V8CodeCache {
    inner: RwLock<V8CodeCacheState>,
}

#[derive(Default)]
struct V8CodeCacheState {
    entries: HashMap<V8CodeCacheKey, Vec<u8>>,
    total_bytes: usize,
}

#[derive(Clone)]
struct V8CodeCacheKey {
    specifier: String,
    kind: u8,
    source_hash: u64,
}

/// Borrowed view of a key. Used so `get_sync` can probe the map without
/// allocating a `String` for the specifier on every lookup (V8 calls into
/// the code cache once per ES module load).
struct V8CodeCacheKeyRef<'a> {
    specifier: &'a str,
    kind: u8,
    source_hash: u64,
}

/// Glue trait that lets `V8CodeCacheKey` and `V8CodeCacheKeyRef<'_>` share
/// a single `Hash`/`Eq` impl via a trait object, enabling `HashMap::get`
/// against a borrowed key without allocation.
trait V8CodeCacheKeyAccess {
    fn parts(&self) -> (&str, u8, u64);
}

impl V8CodeCacheKeyAccess for V8CodeCacheKey {
    fn parts(&self) -> (&str, u8, u64) {
        (&self.specifier, self.kind, self.source_hash)
    }
}

impl V8CodeCacheKeyAccess for V8CodeCacheKeyRef<'_> {
    fn parts(&self) -> (&str, u8, u64) {
        (self.specifier, self.kind, self.source_hash)
    }
}

impl Hash for dyn V8CodeCacheKeyAccess + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (s, k, h) = self.parts();
        s.hash(state);
        k.hash(state);
        h.hash(state);
    }
}

impl PartialEq for dyn V8CodeCacheKeyAccess + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.parts() == other.parts()
    }
}

impl Eq for dyn V8CodeCacheKeyAccess + '_ {}

// HashMap calls `Hash`/`PartialEq` on the stored owned key when computing
// buckets and resolving collisions. Forward through the trait object so the
// owned and borrowed shapes hash + compare identically.
impl Hash for V8CodeCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self as &dyn V8CodeCacheKeyAccess).hash(state);
    }
}

impl PartialEq for V8CodeCacheKey {
    fn eq(&self, other: &Self) -> bool {
        (self as &dyn V8CodeCacheKeyAccess) == (other as &dyn V8CodeCacheKeyAccess)
    }
}

impl Eq for V8CodeCacheKey {}

impl<'a> Borrow<dyn V8CodeCacheKeyAccess + 'a> for V8CodeCacheKey {
    fn borrow(&self) -> &(dyn V8CodeCacheKeyAccess + 'a) {
        self
    }
}

fn kind_to_u8(kind: CodeCacheType) -> u8 {
    match kind {
        CodeCacheType::EsModule => 0,
        CodeCacheType::Script => 1,
    }
}

fn entry_size(specifier: &str, data_len: usize) -> usize {
    size_of::<V8CodeCacheKey>() + specifier.len() + data_len
}

impl CodeCache for V8CodeCache {
    fn get_sync(
        &self,
        specifier: &ModuleSpecifier,
        code_cache_type: CodeCacheType,
        source_hash: u64,
    ) -> Option<Vec<u8>> {
        let inner = self.inner.read().ok()?;
        let key_ref = V8CodeCacheKeyRef {
            specifier: specifier.as_str(),
            kind: kind_to_u8(code_cache_type),
            source_hash,
        };
        inner
            .entries
            .get(&key_ref as &dyn V8CodeCacheKeyAccess)
            .cloned()
    }

    fn set_sync(
        &self,
        specifier: ModuleSpecifier,
        code_cache_type: CodeCacheType,
        source_hash: u64,
        data: &[u8],
    ) {
        let key = V8CodeCacheKey {
            specifier: specifier.to_string(),
            kind: kind_to_u8(code_cache_type),
            source_hash,
        };

        let new_size = entry_size(&key.specifier, data.len());
        if new_size > MAX_V8_CODE_CACHE_BYTES {
            return;
        }

        let Ok(mut inner) = self.inner.write() else {
            return;
        };

        // Probe-then-decide: never drop the previously-cached bytecode unless
        // we're certain the replacement will be admitted. Otherwise a near-full
        // cache could turn a cache hit into a permanent miss for that key.
        let old_size = inner
            .entries
            .get(&key)
            .map(|old| entry_size(&key.specifier, old.len()))
            .unwrap_or(0);
        let bytes_after_remove = inner.total_bytes.saturating_sub(old_size);

        if bytes_after_remove + new_size > MAX_V8_CODE_CACHE_BYTES {
            if old_size > 0 {
                // Replacement wouldn't fit. Keep the existing entry rather
                // than dropping a working cache hit for a write we can't honor.
                return;
            }
            // Fresh insert that doesn't fit at the 100MB ceiling. There's no
            // LRU yet (CPL-264 follow-up), so do the simplest recovery: wipe
            // the cache and let the live action set repopulate naturally.
            // Without this, the cache becomes permanently full and never
            // accepts another action's bytecode.
            inner.entries.clear();
            inner.total_bytes = 0;
        } else if old_size > 0 {
            inner.entries.remove(&key);
            inner.total_bytes = bytes_after_remove;
        }

        inner.total_bytes += new_size;
        inner.entries.insert(key, data.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specifier() -> ModuleSpecifier {
        ModuleSpecifier::parse("file:///test.js").unwrap()
    }

    fn specifier_n(i: usize) -> ModuleSpecifier {
        ModuleSpecifier::parse(&format!("file:///test{i}.js")).unwrap()
    }

    #[test]
    fn round_trip() {
        let cache = V8CodeCache::default();
        let s = specifier();
        assert!(cache.get_sync(&s, CodeCacheType::Script, 42).is_none());
        cache.set_sync(s.clone(), CodeCacheType::Script, 42, b"bytecode");
        assert_eq!(
            cache.get_sync(&s, CodeCacheType::Script, 42).as_deref(),
            Some(b"bytecode".as_ref())
        );
    }

    #[test]
    fn kind_and_hash_are_part_of_key() {
        let cache = V8CodeCache::default();
        let s = specifier();
        cache.set_sync(s.clone(), CodeCacheType::Script, 1, b"a");
        assert!(cache.get_sync(&s, CodeCacheType::EsModule, 1).is_none());
        assert!(cache.get_sync(&s, CodeCacheType::Script, 2).is_none());
    }

    #[test]
    fn set_replaces_existing_entry() {
        let cache = V8CodeCache::default();
        let s = specifier();
        cache.set_sync(s.clone(), CodeCacheType::Script, 1, b"first");
        cache.set_sync(s.clone(), CodeCacheType::Script, 1, b"second");
        assert_eq!(
            cache.get_sync(&s, CodeCacheType::Script, 1).as_deref(),
            Some(b"second".as_ref())
        );
    }

    /// Replacement-larger-than-remaining must NOT drop the existing cached
    /// entry — otherwise a near-full cache turns a cache hit into a
    /// permanent miss for that key.
    #[test]
    fn replacement_too_large_keeps_existing_entry() {
        let cache = V8CodeCache::default();
        // Force a near-full state via a large initial entry.
        let big = vec![0u8; MAX_V8_CODE_CACHE_BYTES - 1024];
        let s1 = specifier_n(1);
        cache.set_sync(s1.clone(), CodeCacheType::Script, 1, &big);
        assert!(cache.get_sync(&s1, CodeCacheType::Script, 1).is_some());

        // Insert a small entry, then attempt to replace it with one that
        // would overflow the budget when summed with the existing big entry.
        let s2 = specifier_n(2);
        cache.set_sync(s2.clone(), CodeCacheType::Script, 1, b"small");
        let too_big_replacement = vec![0u8; 4096];
        cache.set_sync(s2.clone(), CodeCacheType::Script, 1, &too_big_replacement);

        // Existing cache hit for s2 must be preserved, not dropped.
        assert_eq!(
            cache.get_sync(&s2, CodeCacheType::Script, 1).as_deref(),
            Some(b"small".as_ref())
        );
    }

    /// Once the cache is at the 100MB ceiling, a fresh insert must trigger
    /// recovery (clear), not become a permanent rejection.
    #[test]
    fn fresh_insert_at_ceiling_recovers() {
        let cache = V8CodeCache::default();
        let big = vec![0u8; MAX_V8_CODE_CACHE_BYTES - 1024];
        cache.set_sync(specifier_n(1), CodeCacheType::Script, 1, &big);
        assert!(
            cache
                .get_sync(&specifier_n(1), CodeCacheType::Script, 1)
                .is_some()
        );

        // Fresh key that would push past MAX. Existing entries get evicted
        // and the fresh entry is admitted.
        let medium = vec![0u8; 8 * 1024];
        cache.set_sync(specifier_n(2), CodeCacheType::Script, 1, &medium);

        assert!(
            cache
                .get_sync(&specifier_n(1), CodeCacheType::Script, 1)
                .is_none(),
            "old entry should have been evicted"
        );
        assert!(
            cache
                .get_sync(&specifier_n(2), CodeCacheType::Script, 1)
                .is_some(),
            "new entry should have been admitted after recovery"
        );
    }
}
