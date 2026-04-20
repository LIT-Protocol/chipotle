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

use std::collections::HashMap;
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

#[derive(Hash, Eq, PartialEq, Clone)]
struct V8CodeCacheKey {
    specifier: String,
    kind: u8,
    source_hash: u64,
}

fn kind_to_u8(kind: CodeCacheType) -> u8 {
    match kind {
        CodeCacheType::EsModule => 0,
        CodeCacheType::Script => 1,
    }
}

fn entry_size(key: &V8CodeCacheKey, data_len: usize) -> usize {
    size_of::<V8CodeCacheKey>() + key.specifier.capacity() + data_len
}

impl CodeCache for V8CodeCache {
    fn get_sync(
        &self,
        specifier: &ModuleSpecifier,
        code_cache_type: CodeCacheType,
        source_hash: u64,
    ) -> Option<Vec<u8>> {
        let key = V8CodeCacheKey {
            specifier: specifier.to_string(),
            kind: kind_to_u8(code_cache_type),
            source_hash,
        };
        let inner = self.inner.read().ok()?;
        inner.entries.get(&key).cloned()
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

        let size_bytes = entry_size(&key, data.len());
        if size_bytes > MAX_V8_CODE_CACHE_BYTES {
            return;
        }

        let Ok(mut inner) = self.inner.write() else {
            return;
        };

        if let Some(old) = inner.entries.remove(&key) {
            inner.total_bytes = inner
                .total_bytes
                .saturating_sub(entry_size(&key, old.len()));
        }

        if inner.total_bytes + size_bytes > MAX_V8_CODE_CACHE_BYTES {
            return;
        }

        inner.total_bytes += size_bytes;
        inner.entries.insert(key, data.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specifier() -> ModuleSpecifier {
        ModuleSpecifier::parse("file:///test.js").unwrap()
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
}
