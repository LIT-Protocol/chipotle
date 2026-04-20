//! Pool of pre-warmed `MainWorker` instances.
//!
//! Each pooled worker lives on its own OS thread, executes exactly one
//! request, and is then dropped. The pool refills asynchronously: every
//! successful acquire spawns one replacement.
//!
//! Pattern: Cloudflare Workers / Deno Deploy ("isolates are cheap to start
//! from a snapshot, expensive to scrub for reuse, so don't reuse them").
//!
//! # Crash protection
//!
//! Bootstrap runs inside `std::panic::catch_unwind` so a Rust panic during
//! V8 / Deno init won't take down the process. Aborts, segfaults, SIGTRAP
//! and FFI undefined behaviour through V8 are NOT catchable here. The
//! legacy cold path has the same exposure today (see `panic_in_op` ignored
//! test in `lit-actions/tests/it.rs`).
//!
//! # Failure mode (half-open breaker)
//!
//! After `POOL_REFILL_FAILURE_LIMIT` consecutive bootstrap failures the
//! breaker opens for `POOL_BREAKER_COOLDOWN`. While Open, `try_acquire`
//! returns `None` so the dispatcher routes everything to the legacy cold
//! path. After cooldown the breaker enters Half-Open and one trial refill
//! is attempted. Success closes the breaker; failure re-opens it with a
//! fresh cooldown.

use std::any::Any;
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use deno_runtime::tokio_util::create_and_run_current_thread;
use lit_actions_grpc::proto::{ExecuteJsRequest, ExecuteJsResponse};
use lit_observability::channels::TracedReceiver;
use lit_observability::logging::set_request_context;
use tracing::{Instrument, Span, debug, error, warn};

use crate::runtime::{self, ActionCodeCache, PoolSharedState};
use crate::server;

/// Refill failure threshold before the breaker opens.
const POOL_REFILL_FAILURE_LIMIT: usize = 5;
/// Initial backoff between refill attempts (doubled per consecutive failure).
const POOL_REFILL_BACKOFF_BASE_MS: u64 = 50;
/// Maximum backoff between refill attempts.
const POOL_REFILL_BACKOFF_MAX_MS: u64 = 5_000;
/// How long the breaker stays Open before transitioning to Half-Open.
const POOL_BREAKER_COOLDOWN: Duration = Duration::from_secs(60);

/// One request handed off to a pre-warmed worker. Carries everything
/// `execute_with_worker` needs plus the request-context fields the worker
/// thread injects for log correlation.
pub(crate) struct WorkItem {
    pub code: String,
    pub js_params: Option<serde_json::Value>,
    pub auth_context: Option<serde_json::Value>,
    pub http_headers: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    pub outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
    pub inbound_rx: TracedReceiver<ExecuteJsRequest>,
    pub is_test_server: bool,
    pub action_code_cache: ActionCodeCache,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub span: Span,
}

/// Send-side of a per-worker work channel. The channel is `bounded(1)`:
/// each pre-warmed worker accepts exactly one work item and is then
/// dropped, so a `TrySendError::Full` is an invariant violation.
pub(crate) struct WorkerHandle {
    pub work_tx: flume::Sender<WorkItem>,
}

#[derive(Debug, Copy, Clone)]
enum BreakerState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen,
}

/// Counters and breaker state. Public so tests and observability can peek.
pub struct PoolHealth {
    consecutive_refill_failures: AtomicUsize,
    breaker: Mutex<BreakerState>,
    full_errors: AtomicUsize,
    disconnected_misses: AtomicUsize,
    refill_failed: AtomicUsize,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl Default for PoolHealth {
    fn default() -> Self {
        Self {
            consecutive_refill_failures: AtomicUsize::new(0),
            breaker: Mutex::new(BreakerState::Closed),
            full_errors: AtomicUsize::new(0),
            disconnected_misses: AtomicUsize::new(0),
            refill_failed: AtomicUsize::new(0),
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
    }
}

impl PoolHealth {
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
    pub fn misses(&self) -> usize {
        self.misses.load(Ordering::Relaxed)
    }
    pub fn refill_failed(&self) -> usize {
        self.refill_failed.load(Ordering::Relaxed)
    }
    pub fn full_errors(&self) -> usize {
        self.full_errors.load(Ordering::Relaxed)
    }
    pub fn disconnected_misses(&self) -> usize {
        self.disconnected_misses.load(Ordering::Relaxed)
    }
}

pub struct WorkerPool {
    target_size: usize,
    shared: Arc<PoolSharedState>,
    ready_tx: flume::Sender<WorkerHandle>,
    ready_rx: flume::Receiver<WorkerHandle>,
    health: Arc<PoolHealth>,
}

impl WorkerPool {
    pub(crate) fn new(target_size: usize, shared: Arc<PoolSharedState>) -> Arc<Self> {
        // Bound the channel at target_size so an over-eager refill loop
        // can never grow the pool past target. `max(1)` avoids `bounded(0)`
        // (rendezvous) when the pool is disabled (target_size == 0).
        let (ready_tx, ready_rx) = flume::bounded(target_size.max(1));
        Arc::new(Self {
            target_size,
            shared,
            ready_tx,
            ready_rx,
            health: Arc::new(PoolHealth::default()),
        })
    }

    pub fn target_size(&self) -> usize {
        self.target_size
    }

    pub fn health(&self) -> Arc<PoolHealth> {
        self.health.clone()
    }

    /// Spawn `target_size` worker threads. Idempotent in spirit; intended
    /// to be called once at startup after the gRPC socket binds.
    pub(crate) fn warmup(self: &Arc<Self>) {
        if self.target_size == 0 {
            debug!("worker pool warmup skipped: target_size = 0");
            return;
        }
        debug!(
            target_size = self.target_size,
            "worker pool warmup starting"
        );
        for _ in 0..self.target_size {
            self.spawn_worker();
        }
    }

    /// Try to grab a pre-warmed worker. On hit, the pool spawns one
    /// replacement asynchronously (drain-then-refill). On miss returns
    /// `None`; caller must fall through to the legacy cold path.
    pub(crate) fn try_acquire(self: &Arc<Self>) -> Option<WorkerHandle> {
        if self.target_size == 0 {
            return None;
        }
        if !self.breaker_allows_acquire() {
            self.health.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        match self.ready_rx.try_recv() {
            Ok(handle) => {
                self.health.hits.fetch_add(1, Ordering::Relaxed);
                self.spawn_worker();
                Some(handle)
            }
            Err(flume::TryRecvError::Empty) => {
                self.health.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
            Err(flume::TryRecvError::Disconnected) => {
                // Both ends are owned by this pool, so disconnect is
                // unreachable. Log and treat as miss.
                error!("pool ready channel disconnected — this is a bug");
                self.health.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Note a `TrySendError::Full` from the dispatcher: under the
    /// one-shot lifecycle this should never happen (the work channel is
    /// `bounded(1)` and no work has been sent yet). Logged at WARN with a
    /// separate counter so it's visible vs the normal Disconnected race.
    pub(crate) fn note_full_error(&self) {
        self.health.full_errors.fetch_add(1, Ordering::Relaxed);
        warn!("pool worker work channel was full when handing off — invariant violation");
    }

    /// Note a `TrySendError::Disconnected` from the dispatcher: the worker
    /// thread died between publishing its handle and the dispatcher
    /// trying to hand it work. Counted at DEBUG: this is a normal-ish race.
    pub(crate) fn note_disconnected(&self) {
        self.health
            .disconnected_misses
            .fetch_add(1, Ordering::Relaxed);
        debug!("pool worker handle disconnected before work could be sent");
    }

    fn breaker_allows_acquire(self: &Arc<Self>) -> bool {
        let mut guard = match self.health.breaker.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match *guard {
            BreakerState::Closed => true,
            BreakerState::HalfOpen => false,
            BreakerState::Open { opened_at } => {
                if opened_at.elapsed() >= POOL_BREAKER_COOLDOWN {
                    debug!("pool breaker cooldown elapsed; transitioning to half-open");
                    *guard = BreakerState::HalfOpen;
                    drop(guard);
                    // One trial refill from this caller. Result will close
                    // or re-open the breaker.
                    self.spawn_worker();
                }
                false
            }
        }
    }

    fn record_refill_success(&self) {
        self.health
            .consecutive_refill_failures
            .store(0, Ordering::Relaxed);
        let mut guard = match self.health.breaker.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match *guard {
            BreakerState::HalfOpen => {
                debug!("pool breaker trial refill succeeded; closing breaker");
                *guard = BreakerState::Closed;
            }
            BreakerState::Open { .. } => {
                // Not expected on the success path: half-open is the only
                // way a refill runs while the breaker is open. Treat
                // conservatively as Closed.
                *guard = BreakerState::Closed;
            }
            BreakerState::Closed => {}
        }
    }

    fn record_refill_failure(&self, err: &dyn std::fmt::Display) {
        self.health.refill_failed.fetch_add(1, Ordering::Relaxed);
        let prev = self
            .health
            .consecutive_refill_failures
            .fetch_add(1, Ordering::Relaxed);
        let total_failures = prev + 1;
        let mut guard = match self.health.breaker.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let was_half_open = matches!(*guard, BreakerState::HalfOpen);
        if total_failures >= POOL_REFILL_FAILURE_LIMIT || was_half_open {
            error!(
                consecutive_failures = total_failures,
                error = %err,
                "pool refill breaker opening — pool degraded; falling back to legacy cold path until cooldown ({POOL_BREAKER_COOLDOWN:?})",
            );
            *guard = BreakerState::Open {
                opened_at: Instant::now(),
            };
        } else {
            warn!(
                consecutive_failures = total_failures,
                error = %err,
                "pool worker bootstrap failed; will retry with backoff",
            );
        }
    }

    fn spawn_worker(self: &Arc<Self>) {
        let pool = self.clone();
        std::thread::spawn(move || {
            run_worker_thread(pool);
        });
    }
}

fn run_worker_thread(pool: Arc<WorkerPool>) {
    // Bootstrap is the panic surface: any V8 / Deno init issue lands here.
    // catch_unwind protects against Rust panics; aborts/segfaults/SIGTRAP
    // and FFI UB through V8 are NOT catchable here.
    let bootstrap_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        runtime::build_worker_base(&pool.shared)
    }));

    let prepared = match bootstrap_result {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(err)) => {
            pool.record_refill_failure(&format_args!("{err:#}"));
            schedule_replacement(&pool);
            return;
        }
        Err(panic) => {
            let msg = panic_message(&panic);
            pool.record_refill_failure(&msg);
            schedule_replacement(&pool);
            return;
        }
    };

    pool.record_refill_success();

    let (work_tx, work_rx) = flume::bounded::<WorkItem>(1);
    let handle = WorkerHandle { work_tx };

    if pool.ready_tx.send(handle).is_err() {
        // Pool was dropped before this worker could publish itself.
        return;
    }

    // Block until either work arrives or the dispatcher gives up on this
    // handle (drops it / panics).
    let work = match work_rx.recv() {
        Ok(work) => work,
        Err(_) => {
            // Dispatcher disconnected without sending work; just exit.
            return;
        }
    };

    // From here on we run a single request and drop the worker.
    if work.request_id.is_some() || work.correlation_id.is_some() {
        set_request_context(work.request_id.clone(), work.correlation_id.clone());
    }

    let WorkItem {
        code,
        js_params,
        auth_context,
        http_headers,
        timeout_ms,
        outbound_tx,
        inbound_rx,
        is_test_server,
        action_code_cache,
        request_id: _,
        correlation_id: _,
        span,
    } = work;

    let shared = pool.shared.clone();
    let mut prepared = prepared;

    create_and_run_current_thread(
        async move {
            // Per-request JS injection: namespace globals first (so user
            // code sees Lit.*), PatchDeno + everything else is owned by
            // execute_with_worker — preserves today's bootstrap order.
            if let Err(err) =
                runtime::inject_lit_namespace(&mut prepared.worker, &auth_context, &http_headers)
            {
                error!("pool worker failed to inject Lit namespace: {err:#}");
                server::send_execution_result(&outbound_tx, Err(err)).await;
                return;
            }

            let res = runtime::execute_with_worker(
                prepared,
                shared,
                code,
                js_params,
                http_headers,
                timeout_ms,
                outbound_tx.clone(),
                inbound_rx,
                is_test_server,
                action_code_cache,
            )
            .await;

            server::send_execution_result(&outbound_tx, res).await;
        }
        .instrument(span),
    );
    // MainWorker dropped here; thread exits.
}

fn schedule_replacement(pool: &Arc<WorkerPool>) {
    let failures = pool
        .health
        .consecutive_refill_failures
        .load(Ordering::Relaxed)
        .max(1);
    // Exponential backoff, capped: 50ms, 100, 200, 400, 800, 1600, 3200, 5000ms.
    // First failure (failures == 1) → BASE_MS << 0 = 50ms.
    // Shift bounded to 7 — shifting a u64 by >=64 bits is UB (panics in debug).
    let shift = (failures - 1).min(7);
    let backoff_ms = (POOL_REFILL_BACKOFF_BASE_MS << shift).min(POOL_REFILL_BACKOFF_MAX_MS);
    let pool = pool.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(backoff_ms));
        // Recheck breaker before issuing the replacement: if Open and not
        // yet cooled down, skip — the breaker will issue a half-open trial
        // when a request next arrives.
        let allow = match pool.health.breaker.lock() {
            Ok(g) => matches!(*g, BreakerState::Closed | BreakerState::HalfOpen),
            Err(p) => matches!(
                *p.into_inner(),
                BreakerState::Closed | BreakerState::HalfOpen
            ),
        };
        if !allow {
            return;
        }
        pool.spawn_worker();
    });
}

fn panic_message(panic: &Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;

    use crate::cdn_module_loader::CdnModuleLoader;
    use crate::runtime::DEFAULT_MEMORY_LIMIT_MB;

    use super::*;

    fn test_shared() -> Arc<PoolSharedState> {
        Arc::new(PoolSharedState {
            integrity_manifest: Arc::new(RwLock::new(HashMap::new())),
            strict_imports: false,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            lockfile_path: None,
            http_client: CdnModuleLoader::build_http_client(),
            memory_limit_mb: DEFAULT_MEMORY_LIMIT_MB,
            v8_code_cache: crate::v8_code_cache::new_v8_code_cache(),
        })
    }

    #[test]
    fn try_acquire_returns_none_when_pool_disabled() {
        let pool = WorkerPool::new(0, test_shared());
        assert!(pool.try_acquire().is_none());
        // Disabled pool short-circuits before counters: no hits, no misses.
        assert_eq!(pool.health().hits(), 0);
        assert_eq!(pool.health().misses(), 0);
    }

    #[test]
    fn note_full_error_increments_full_counter() {
        let pool = WorkerPool::new(0, test_shared());
        pool.note_full_error();
        pool.note_full_error();
        assert_eq!(pool.health().full_errors(), 2);
        assert_eq!(pool.health().disconnected_misses(), 0);
    }

    #[test]
    fn note_disconnected_increments_disconnected_counter() {
        let pool = WorkerPool::new(0, test_shared());
        pool.note_disconnected();
        assert_eq!(pool.health().disconnected_misses(), 1);
        assert_eq!(pool.health().full_errors(), 0);
    }

    #[test]
    fn breaker_opens_after_consecutive_failures() {
        let pool = WorkerPool::new(1, test_shared());
        for _ in 0..POOL_REFILL_FAILURE_LIMIT {
            pool.record_refill_failure(&"synthetic bootstrap failure");
        }
        assert!(matches!(
            *pool.health.breaker.lock().unwrap(),
            BreakerState::Open { .. }
        ));
        assert_eq!(pool.health().refill_failed(), POOL_REFILL_FAILURE_LIMIT);
        // Breaker open => try_acquire returns None and counts a miss.
        assert!(pool.try_acquire().is_none());
        assert_eq!(pool.health().misses(), 1);
    }

    #[test]
    fn breaker_half_open_failure_reopens_with_fresh_cooldown() {
        let pool = WorkerPool::new(1, test_shared());
        // Manually park breaker in HalfOpen.
        *pool.health.breaker.lock().unwrap() = BreakerState::HalfOpen;
        // A single failure from HalfOpen MUST re-open even under the
        // consecutive-failure threshold.
        pool.record_refill_failure(&"trial bootstrap failure");
        assert!(matches!(
            *pool.health.breaker.lock().unwrap(),
            BreakerState::Open { .. }
        ));
    }

    #[test]
    fn breaker_half_open_success_closes() {
        let pool = WorkerPool::new(1, test_shared());
        *pool.health.breaker.lock().unwrap() = BreakerState::HalfOpen;
        pool.record_refill_success();
        assert!(matches!(
            *pool.health.breaker.lock().unwrap(),
            BreakerState::Closed
        ));
    }

    #[test]
    fn record_refill_success_resets_failure_counter() {
        let pool = WorkerPool::new(1, test_shared());
        pool.record_refill_failure(&"first");
        pool.record_refill_failure(&"second");
        assert_eq!(
            pool.health
                .consecutive_refill_failures
                .load(Ordering::Relaxed),
            2
        );
        pool.record_refill_success();
        assert_eq!(
            pool.health
                .consecutive_refill_failures
                .load(Ordering::Relaxed),
            0
        );
    }
}
