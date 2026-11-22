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
//!
//! Entries are keyed by source and specifier. Each worker opens its cache
//! gate for the server-managed compilation of the action.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use deno_core::ModuleSpecifier;
use deno_runtime::code_cache::{CodeCache, CodeCacheType};
use sha2::{Digest, Sha256};

const MAX_V8_CODE_CACHE_BYTES: usize = 100 * 1024 * 1024;

pub type SharedV8CodeCache = Arc<V8CodeCache>;

pub fn new_v8_code_cache() -> SharedV8CodeCache {
    Arc::new(V8CodeCache::default())
}

/// Identity of one compiled script: a SHA-256 over the specifier it is compiled
/// under and its full source text. Always computed in Rust from the source the
/// server is about to run, never taken from the isolate.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CodeIdentity([u8; 32]);

impl CodeIdentity {
    pub fn new(specifier: &str, source: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"lit-actions/v8-code-cache/v1");
        // Length-prefix the specifier so (specifier, source) splits unambiguously.
        hasher.update((specifier.len() as u64).to_le_bytes());
        hasher.update(specifier.as_bytes());
        hasher.update(source.as_bytes());
        Self(hasher.finalize().into())
    }
}

#[derive(Default)]
pub struct V8CodeCache {
    inner: RwLock<V8CodeCacheState>,
    /// Lookups that returned cached bytecode / that missed. Observability
    /// only — lets tests see that cached bytecode was offered to V8 on a
    /// repeat execution (vs. compiled from scratch every run).
    hits: AtomicU64,
    misses: AtomicU64,
}

#[derive(Default)]
struct V8CodeCacheState {
    entries: HashMap<CodeIdentity, Vec<u8>>,
    total_bytes: usize,
}

fn entry_size(data_len: usize) -> usize {
    size_of::<CodeIdentity>() + data_len
}

impl V8CodeCache {
    /// Number of lookups served from cache, i.e. cached bytecode handed to V8.
    /// V8 can still reject a blob and recompile; that is not visible here. See
    /// [`Self::misses`].
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Number of lookups that found nothing (forcing a fresh compile, whose
    /// result is then stored).
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.inner.read().map(|s| s.entries.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, identity: &CodeIdentity) -> Option<Vec<u8>> {
        let hit = self.inner.read().ok()?.entries.get(identity).cloned();
        if hit.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }

    fn set(&self, identity: CodeIdentity, data: &[u8]) {
        let new_size = entry_size(data.len());
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
            .get(&identity)
            .map(|old| entry_size(old.len()))
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
            inner.entries.remove(&identity);
            inner.total_bytes = bytes_after_remove;
        }

        inner.total_bytes += new_size;
        inner.entries.insert(identity, data.to_vec());
    }
}

/// One worker's only route to the shared [`V8CodeCache`]. This — not the shared
/// cache — is what gets wired into the worker as Deno's [`CodeCache`].
///
/// Closed by default: every lookup misses and every store is dropped, so
/// compiles triggered from inside the isolate never read or write shared
/// state. [`Self::arm`] opens it for exactly one compile of one known source;
/// the lookup for that compile closes it again. Deno performs the lookup, the
/// compile and the store for a script before running any of it, so the gate is
/// closed again by the time the first line of action code executes.
pub struct WorkerCodeCacheGate {
    shared: SharedV8CodeCache,
    state: Mutex<GateState>,
}

enum GateState {
    Closed,
    /// The server is about to compile this source under this specifier.
    Armed {
        specifier: String,
        identity: CodeIdentity,
    },
    /// The armed compile missed; its bytecode is expected next.
    AwaitingStore {
        specifier: String,
        identity: CodeIdentity,
        source_hash: u64,
    },
}

impl WorkerCodeCacheGate {
    pub fn new(shared: SharedV8CodeCache) -> Self {
        Self {
            shared,
            state: Mutex::new(GateState::Closed),
        }
    }

    /// Open the gate for the next eval-context compile, which must be of
    /// `source` under `specifier`. Call immediately before triggering that
    /// compile, and [`Self::close`] once it has returned.
    pub fn arm(&self, specifier: &str, source: &str) {
        self.replace_state(GateState::Armed {
            specifier: specifier.to_string(),
            identity: CodeIdentity::new(specifier, source),
        });
    }

    /// Close the gate. Idempotent; a backstop for an armed compile that never
    /// reached its lookup or store (e.g. the source failed to parse).
    pub fn close(&self) {
        self.replace_state(GateState::Closed);
    }

    fn replace_state(&self, next: GateState) -> GateState {
        // A poisoned lock still guards a valid enum; keep using it so the gate
        // can always be closed.
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::replace(&mut state, next)
    }
}

impl CodeCache for WorkerCodeCacheGate {
    fn get_sync(
        &self,
        specifier: &ModuleSpecifier,
        code_cache_type: CodeCacheType,
        source_hash: u64,
    ) -> Option<Vec<u8>> {
        // Any lookup closes the gate, whether or not it was the armed one.
        let GateState::Armed {
            specifier: armed_specifier,
            identity,
        } = self.replace_state(GateState::Closed)
        else {
            return None;
        };
        if code_cache_type != CodeCacheType::Script || specifier.as_str() != armed_specifier {
            return None;
        }

        let hit = self.shared.get(&identity);
        // Only a miss expects a store. On a hit the gate stays closed, so no
        // pending state is left open while the script runs. The cost: if V8
        // were to reject the blob, Deno's replacement store is dropped and that
        // action recompiles each run until the entry is evicted. Blobs here are
        // produced and consumed by the same V8 build and flags, for the same
        // source, so rejection is not expected.
        if hit.is_none() {
            self.replace_state(GateState::AwaitingStore {
                specifier: armed_specifier,
                identity,
                source_hash,
            });
        }
        hit
    }

    fn set_sync(
        &self,
        specifier: ModuleSpecifier,
        code_cache_type: CodeCacheType,
        source_hash: u64,
        data: &[u8],
    ) {
        let GateState::AwaitingStore {
            specifier: armed_specifier,
            identity,
            source_hash: armed_source_hash,
        } = self.replace_state(GateState::Closed)
        else {
            return;
        };
        if code_cache_type != CodeCacheType::Script
            || specifier.as_str() != armed_specifier
            || source_hash != armed_source_hash
        {
            return;
        }
        self.shared.set(identity, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPECIFIER: &str = "file:///test.js";

    fn url(specifier: &str) -> ModuleSpecifier {
        ModuleSpecifier::parse(specifier).unwrap()
    }

    fn id(source: &str) -> CodeIdentity {
        CodeIdentity::new(SPECIFIER, source)
    }

    fn lookup(gate: &WorkerCodeCacheGate, specifier: &str) -> Option<Vec<u8>> {
        gate.get_sync(&url(specifier), CodeCacheType::Script, 7)
    }

    fn store(gate: &WorkerCodeCacheGate, specifier: &str, data: &[u8]) {
        gate.set_sync(url(specifier), CodeCacheType::Script, 7, data);
    }

    #[test]
    fn round_trip() {
        let cache = V8CodeCache::default();
        assert!(cache.get(&id("a")).is_none());
        cache.set(id("a"), b"bytecode");
        assert_eq!(cache.get(&id("a")).as_deref(), Some(b"bytecode".as_ref()));
    }

    /// Identity covers the full source text and the specifier.
    #[test]
    fn identity_covers_source_and_specifier() {
        assert!(id("aaaa") != id("aaab"));
        assert!(CodeIdentity::new("file:///a.js", "x") != CodeIdentity::new("file:///b.js", "x"));
        // The specifier/source boundary is unambiguous.
        assert!(CodeIdentity::new("ab", "c") != CodeIdentity::new("a", "bc"));

        let cache = V8CodeCache::default();
        cache.set(id("aaaa"), b"bytecode-a");
        cache.set(id("aaab"), b"bytecode-b");
        assert_eq!(
            cache.get(&id("aaaa")).as_deref(),
            Some(b"bytecode-a".as_ref())
        );
        assert_eq!(
            cache.get(&id("aaab")).as_deref(),
            Some(b"bytecode-b".as_ref())
        );
    }

    #[test]
    fn hit_and_miss_counters_track_lookups() {
        let cache = V8CodeCache::default();
        assert_eq!((cache.hits(), cache.misses()), (0, 0));

        // Miss: nothing stored yet.
        assert!(cache.get(&id("a")).is_none());
        assert_eq!((cache.hits(), cache.misses()), (0, 1));

        // Store, then hit.
        cache.set(id("a"), b"bytecode");
        assert!(cache.get(&id("a")).is_some());
        assert_eq!((cache.hits(), cache.misses()), (1, 1));
    }

    #[test]
    fn set_replaces_existing_entry() {
        let cache = V8CodeCache::default();
        cache.set(id("a"), b"first");
        cache.set(id("a"), b"second");
        assert_eq!(cache.get(&id("a")).as_deref(), Some(b"second".as_ref()));
        assert_eq!(cache.len(), 1);
    }

    /// Replacement-larger-than-remaining must NOT drop the existing cached
    /// entry — otherwise a near-full cache turns a cache hit into a
    /// permanent miss for that key.
    #[test]
    fn replacement_too_large_keeps_existing_entry() {
        let cache = V8CodeCache::default();
        // Force a near-full state via a large initial entry.
        let big = vec![0u8; MAX_V8_CODE_CACHE_BYTES - 1024];
        cache.set(id("big"), &big);
        assert!(cache.get(&id("big")).is_some());

        // Insert a small entry, then attempt to replace it with one that
        // would overflow the budget when summed with the existing big entry.
        cache.set(id("small"), b"small");
        let too_big_replacement = vec![0u8; 4096];
        cache.set(id("small"), &too_big_replacement);

        // Existing cache hit must be preserved, not dropped.
        assert_eq!(cache.get(&id("small")).as_deref(), Some(b"small".as_ref()));
    }

    /// Once the cache is at the 100MB ceiling, a fresh insert must trigger
    /// recovery (clear), not become a permanent rejection.
    #[test]
    fn fresh_insert_at_ceiling_recovers() {
        let cache = V8CodeCache::default();
        let big = vec![0u8; MAX_V8_CODE_CACHE_BYTES - 1024];
        cache.set(id("big"), &big);
        assert!(cache.get(&id("big")).is_some());

        // Fresh key that would push past MAX. Existing entries get evicted
        // and the fresh entry is admitted.
        let medium = vec![0u8; 8 * 1024];
        cache.set(id("medium"), &medium);

        assert!(
            cache.get(&id("big")).is_none(),
            "old entry should have been evicted"
        );
        assert!(
            cache.get(&id("medium")).is_some(),
            "new entry should have been admitted after recovery"
        );
    }

    fn gate() -> (SharedV8CodeCache, WorkerCodeCacheGate) {
        let shared = new_v8_code_cache();
        (shared.clone(), WorkerCodeCacheGate::new(shared))
    }

    /// The armed compile: miss, store, then a second worker's armed compile of
    /// the same source hits.
    #[test]
    fn armed_compile_stores_and_is_reused() {
        let (shared, gate) = gate();
        gate.arm(SPECIFIER, "source");
        assert!(lookup(&gate, SPECIFIER).is_none());
        store(&gate, SPECIFIER, b"bytecode");
        assert_eq!(shared.len(), 1);

        let other_worker = WorkerCodeCacheGate::new(shared.clone());
        other_worker.arm(SPECIFIER, "source");
        assert_eq!(
            lookup(&other_worker, SPECIFIER).as_deref(),
            Some(b"bytecode".as_ref())
        );
    }

    /// A gate that was never armed is inert: nothing is read or written, even
    /// for the specifier and hash of an entry that exists.
    #[test]
    fn unarmed_gate_neither_reads_nor_writes() {
        let (shared, gate) = gate();
        shared.set(id("source"), b"bytecode");

        assert!(lookup(&gate, SPECIFIER).is_none());
        store(&gate, SPECIFIER, b"second");

        assert_eq!(shared.len(), 1);
        assert_eq!(
            shared.get(&id("source")).as_deref(),
            Some(b"bytecode".as_ref())
        );
    }

    /// After the armed compile — hit or miss — later compiles in the same
    /// isolate get nothing, including ones reusing its specifier and hash.
    #[test]
    fn gate_closes_after_the_armed_compile() {
        // Miss path: lookup, store, then closed.
        let (shared, gate) = gate();
        gate.arm(SPECIFIER, "source");
        assert!(lookup(&gate, SPECIFIER).is_none());
        store(&gate, SPECIFIER, b"bytecode");

        assert!(lookup(&gate, SPECIFIER).is_none());
        store(&gate, SPECIFIER, b"second");
        assert_eq!(
            shared.get(&id("source")).as_deref(),
            Some(b"bytecode".as_ref())
        );

        // Hit path: closed straight after the lookup.
        let hit_gate = WorkerCodeCacheGate::new(shared.clone());
        hit_gate.arm(SPECIFIER, "source");
        assert!(lookup(&hit_gate, SPECIFIER).is_some());
        store(&hit_gate, SPECIFIER, b"second");
        assert!(lookup(&hit_gate, SPECIFIER).is_none());
        assert_eq!(
            shared.get(&id("source")).as_deref(),
            Some(b"bytecode".as_ref())
        );
    }

    /// If the first compile after arming is not the armed one, the gate closes
    /// without touching the shared cache.
    #[test]
    fn mismatched_compile_closes_the_gate() {
        let (shared, gate) = gate();
        shared.set(id("source"), b"bytecode");

        gate.arm(SPECIFIER, "source");
        assert!(lookup(&gate, "file:///other.js").is_none());
        // The armed specifier no longer works either.
        assert!(lookup(&gate, SPECIFIER).is_none());

        gate.arm(SPECIFIER, "source");
        assert!(
            gate.get_sync(&url(SPECIFIER), CodeCacheType::EsModule, 7)
                .is_none()
        );

        // A store that doesn't match the pending lookup is dropped.
        gate.arm(SPECIFIER, "fresh");
        assert!(lookup(&gate, SPECIFIER).is_none());
        gate.set_sync(url(SPECIFIER), CodeCacheType::Script, 8, b"second");
        gate.arm(SPECIFIER, "fresh");
        assert!(lookup(&gate, SPECIFIER).is_none());
        store(&gate, "file:///other.js", b"second");

        assert_eq!(shared.len(), 1);
    }

    #[test]
    fn close_discards_a_pending_store() {
        let (shared, gate) = gate();
        gate.arm(SPECIFIER, "source");
        assert!(lookup(&gate, SPECIFIER).is_none());
        gate.close();
        store(&gate, SPECIFIER, b"bytecode");
        assert!(shared.is_empty());

        gate.arm(SPECIFIER, "source");
        gate.close();
        assert!(lookup(&gate, SPECIFIER).is_none());
    }
}
