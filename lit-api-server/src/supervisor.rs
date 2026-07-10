//! In-process supervisor for long-lived background tasks **inside the TEE**.
//!
//! # Why in-process recovery (not "let it crash")
//!
//! Outside an enclave the correct move for a dead background thread is usually a
//! loud process exit so an external supervisor (systemd, k8s) restarts it. Inside
//! the dstack / Phala enclave a full process exit is roughly three orders of
//! magnitude more expensive: it forces re-attestation, re-derivation of sealed key
//! material, and possible removal from the active node set. Restarts here are meant
//! to be deliberate, coordinated, on-chain events.
//!
//! So for the non-critical background actors we invert the usual advice: catch the
//! failure, re-spawn the task with backoff, and keep the enclave warm. This mirrors
//! the pattern `lit-actions/server/worker_pool.rs` already proves works in the TEE
//! (catch_unwind + circuit breaker + auto-replacement). We intentionally do **not**
//! converge onto that module — it supervises OS threads running one-shot workers on
//! a current-thread runtime, a genuinely different abstraction from a generic
//! async-task supervisor.
//!
//! # What this catches — and what it does NOT
//!
//! [`supervise`] catches task **death**: a clean Rust panic (via the spawned
//! `JoinHandle`'s `Err(JoinError)`) or the future returning. It does **not** catch:
//!
//! - **Wedges** — a stuck RPC await, a livelock, a stuck reply path. The future
//!   never returns, so `JoinHandle` never observes it. The supervised loops here
//!   already `match`-and-continue on transient errors and run forever, so a wedge is
//!   the *more likely* failure. The [`spawn_watchdog`] heartbeat-staleness check is
//!   the real defense against wedges, not the panic catcher.
//! - **Aborts / segfaults / SIGTRAP / FFI UB** through V8 or signing libs. Like
//!   `catch_unwind`, `JoinError` recovers clean Rust panics only; these still take
//!   the whole process down. The supervisor is a guard against logic panics in a
//!   loop body, not a total safety net.
//!
//! Panic recovery requires `panic = "unwind"` (the default; verified that no
//! workspace profile sets `panic = "abort"`).

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds since the Unix epoch, used for heartbeat timestamps.
///
/// Wall-clock (not a monotonic `Instant`) so the watchdog can compute staleness
/// with a plain subtraction and the value is meaningful in logs. A backwards clock
/// step only risks a spurious staleness alert, never a missed task death.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Per-task health, published by [`supervise`] and read by [`spawn_watchdog`].
pub struct TaskState {
    name: String,
    /// Wall-clock ms of the most recent heartbeat (a loop iteration or a (re)spawn).
    heartbeat_ms: AtomicU64,
    /// `true` while the circuit breaker is open — a persistent failure the
    /// supervisor could not recover. The task is still retried on the cooldown
    /// cadence; the flag is purely an alarm signal.
    breaker_open: AtomicBool,
}

impl TaskState {
    /// The task's stable name (also used as a metric/label).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Record a liveness heartbeat. Call once per loop iteration from **inside**
    /// the supervised task — this is what lets the watchdog detect a wedge that
    /// [`supervise`] cannot.
    pub fn beat(&self) {
        self.heartbeat_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// Wall-clock ms of the most recent heartbeat.
    pub fn heartbeat_ms(&self) -> u64 {
        self.heartbeat_ms.load(Ordering::Relaxed)
    }

    /// Whether the circuit breaker is currently open (task degraded).
    pub fn breaker_open(&self) -> bool {
        self.breaker_open.load(Ordering::Relaxed)
    }

    fn set_breaker_open(&self, open: bool) {
        self.breaker_open.store(open, Ordering::Relaxed);
    }
}

/// Cloneable registry of supervised-task health.
#[derive(Clone, Default)]
pub struct TaskHealth {
    tasks: Arc<Mutex<HashMap<String, Arc<TaskState>>>>,
}

impl TaskHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or fetch) the [`TaskState`] for `name`, seeding the heartbeat to
    /// now so a freshly registered task is never immediately considered stale.
    pub fn register(&self, name: &str) -> Arc<TaskState> {
        let mut tasks = self.tasks.lock().unwrap_or_else(|p| p.into_inner());
        tasks
            .entry(name.to_string())
            .or_insert_with(|| {
                Arc::new(TaskState {
                    name: name.to_string(),
                    heartbeat_ms: AtomicU64::new(now_ms()),
                    breaker_open: AtomicBool::new(false),
                })
            })
            .clone()
    }

    /// Snapshot of every registered task's state handle.
    pub fn tasks(&self) -> Vec<Arc<TaskState>> {
        self.tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Pure staleness/degradation check used by the watchdog (and unit tests).
    ///
    /// Returns a report for every task whose heartbeat is older than
    /// `threshold_ms` relative to `now_ms_val` (a wedge) **or** whose breaker is
    /// open (a persistent death the supervisor keeps retrying).
    pub fn unhealthy(&self, now_ms_val: u64, threshold_ms: u64) -> Vec<TaskReport> {
        self.tasks()
            .iter()
            .filter_map(|t| {
                let hb = t.heartbeat_ms();
                let stale = now_ms_val.saturating_sub(hb) > threshold_ms;
                let breaker_open = t.breaker_open();
                (stale || breaker_open).then(|| TaskReport {
                    name: t.name.clone(),
                    last_heartbeat_ms: hb,
                    stale,
                    breaker_open,
                })
            })
            .collect()
    }
}

/// One unhealthy task as seen by the watchdog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskReport {
    pub name: String,
    pub last_heartbeat_ms: u64,
    pub stale: bool,
    pub breaker_open: bool,
}

/// Backoff + circuit-breaker policy for a supervised task.
#[derive(Debug, Clone, Copy)]
pub struct SupervisorPolicy {
    /// First backoff after a failure; doubles per consecutive failure.
    pub backoff_base: Duration,
    /// Cap on the per-failure backoff.
    pub backoff_max: Duration,
    /// Consecutive failures before the breaker opens (degraded + alert).
    pub failure_limit: u32,
    /// While the breaker is open, the cadence at which re-spawn is retried.
    pub cooldown: Duration,
    /// A task that stayed up at least this long before dying is treated as a
    /// healthy run: the consecutive-failure counter resets and the breaker closes.
    /// This is what turns a long-lived task's eventual death into "transient" and
    /// makes the post-cooldown re-spawn a true half-open trial.
    pub healthy_after: Duration,
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        // Mirrors lit-actions worker_pool: 50 ms → 5 s backoff, 5-failure breaker,
        // 60 s cooldown.
        Self {
            backoff_base: Duration::from_millis(50),
            backoff_max: Duration::from_secs(5),
            failure_limit: 5,
            cooldown: Duration::from_secs(60),
            healthy_after: Duration::from_secs(30),
        }
    }
}

/// Capped exponential backoff for the `n`-th consecutive failure (1-based).
///
/// 50 ms, 100, 200, 400, 800, 1600, 3200, 5000 (capped). Shift is bounded to 7 —
/// shifting a `u64` by ≥64 bits is UB (panics in debug). Mirrors
/// `worker_pool::schedule_replacement`.
fn backoff_for(consecutive_failures: u32, policy: &SupervisorPolicy) -> Duration {
    let shift = consecutive_failures.saturating_sub(1).min(7);
    let base_ms = policy.backoff_base.as_millis() as u64;
    let max_ms = policy.backoff_max.as_millis() as u64;
    Duration::from_millis(base_ms.saturating_mul(1u64 << shift).min(max_ms))
}

/// Spawn `factory` under supervision: on completion **or panic**, re-run `factory`
/// with capped exponential backoff and a circuit breaker. **Never exits the
/// process** — that is reserved for the existing fatal cases (Rocket launch/bind
/// failure and a Rocket-task panic).
///
/// `factory` receives the task's [`TaskState`] so the loop body can [`TaskState::beat`]
/// each iteration. The heartbeat is the watchdog's wedge signal; the re-spawn path
/// here only handles death (see the module docs).
///
/// On the `failure_limit`-th consecutive failure the breaker opens: the task is
/// marked degraded in [`TaskHealth`], an alert is emitted, and re-spawn continues on
/// the cooldown cadence (we degrade + alert, we do not give up). A task that runs
/// longer than `healthy_after` resets the failure count and closes the breaker, so
/// the post-cooldown re-spawn behaves as a half-open trial.
pub fn supervise<F, Fut>(name: &str, health: TaskHealth, policy: SupervisorPolicy, factory: F)
where
    F: Fn(Arc<TaskState>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let name = name.to_string();
    let state = health.register(&name);

    tokio::spawn(async move {
        let mut consecutive_failures: u32 = 0;

        loop {
            // A (re)spawn is itself a liveness signal, so the watchdog doesn't flag
            // a task as stale during its backoff window.
            state.beat();

            let started = tokio::time::Instant::now();
            // Spawn the factory's future as its OWN task: a panic inside it is
            // isolated and surfaces as `Err(JoinError)` here, instead of unwinding
            // through this supervisor. We do NOT `catch_unwind` the future directly —
            // async state machines aren't `UnwindSafe` and `AssertUnwindSafe` across
            // await points is a footgun.
            let outcome = tokio::spawn(factory(state.clone())).await;
            let ran = started.elapsed();

            match &outcome {
                Ok(()) => {
                    tracing::warn!(
                        task = %name,
                        ran_secs = ran.as_secs_f64(),
                        "supervised task returned unexpectedly; re-spawning"
                    );
                }
                Err(e) if e.is_panic() => {
                    tracing::error!(
                        task = %name,
                        ran_secs = ran.as_secs_f64(),
                        "supervised task PANICKED; re-spawning (enclave kept warm)"
                    );
                }
                Err(_) => {
                    // Task was cancelled — only happens on runtime shutdown. Stop
                    // supervising rather than hot-loop against a dying runtime.
                    tracing::info!(task = %name, "supervised task cancelled; supervisor stopping");
                    return;
                }
            }

            // A task that stayed up long enough counts as recovered: this death is
            // an isolated incident, not a crash loop.
            if ran >= policy.healthy_after {
                consecutive_failures = 0;
                if state.breaker_open() {
                    state.set_breaker_open(false);
                    tracing::info!(task = %name, "supervised task recovered; breaker closed");
                }
            }

            consecutive_failures += 1;

            if consecutive_failures >= policy.failure_limit {
                // Count only the closed→open transition, not every failure past
                // the limit — otherwise the counter tracks failures-while-open
                // rather than breaker openings and reads misleadingly high.
                if !state.breaker_open() {
                    state.set_breaker_open(true);
                    metrics::counter!("supervisor.breaker_open_total", "task" => name.clone())
                        .increment(1);
                }
                tracing::error!(
                    task = %name,
                    consecutive_failures,
                    cooldown_secs = policy.cooldown.as_secs(),
                    "supervised task breaker OPEN — degraded; retrying on cooldown cadence"
                );
                tokio::time::sleep(policy.cooldown).await;
            } else {
                let backoff = backoff_for(consecutive_failures, &policy);
                tracing::warn!(
                    task = %name,
                    consecutive_failures,
                    backoff_ms = backoff.as_millis() as u64,
                    "re-spawning supervised task after backoff"
                );
                tokio::time::sleep(backoff).await;
            }
        }
    });
}

/// Cadence + staleness threshold for [`spawn_watchdog`].
#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// How often to scan the registry.
    pub poll_interval: Duration,
    /// A heartbeat older than this is reported stale. **Must exceed the slowest
    /// supervised loop's period with margin** — the chain-config refresh beats
    /// every 30 s, so the default is well above that.
    pub staleness_threshold: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(15),
            staleness_threshold: Duration::from_secs(90),
        }
    }
}

/// Spawn the heartbeat-staleness watchdog. This is the **primary** failure signal:
/// it catches wedges (which [`supervise`] cannot) as well as open breakers.
///
/// Observability only — by design it does not exit the process or flip `/health`.
/// In the Phala deployment the gateway serves the domain from exactly one instance
/// (no load balancer to drain to), so an automated restart has no safe actuator;
/// a human or the existing on-chain `ServerTriggered` path decides if a real
/// restart is warranted.
pub fn spawn_watchdog(health: TaskHealth, config: WatchdogConfig) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.poll_interval);
        let threshold_ms = config.staleness_threshold.as_millis() as u64;

        loop {
            interval.tick().await;

            let now = now_ms();
            let unhealthy = health.unhealthy(now, threshold_ms);
            let unhealthy_names: std::collections::HashSet<&str> =
                unhealthy.iter().map(|r| r.name.as_str()).collect();

            // Publish a per-task gauge (1 = unhealthy, 0 = healthy) so the signal is
            // visible even between log lines.
            for task in health.tasks() {
                let value = if unhealthy_names.contains(task.name()) {
                    1.0
                } else {
                    0.0
                };
                metrics::gauge!("supervisor.task_unhealthy", "task" => task.name().to_string())
                    .set(value);
            }

            for report in unhealthy {
                tracing::error!(
                    task = %report.name,
                    stale = report.stale,
                    breaker_open = report.breaker_open,
                    last_heartbeat_ms = report.last_heartbeat_ms,
                    age_ms = now.saturating_sub(report.last_heartbeat_ms),
                    "supervised task UNHEALTHY (stale heartbeat or open breaker) — investigate"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    /// A policy with tiny durations so the supervisor's backoff/cooldown loop runs
    /// fast in tests, and `healthy_after` is large so quick failures never count as
    /// healthy runs.
    fn fast_policy() -> SupervisorPolicy {
        SupervisorPolicy {
            backoff_base: Duration::from_millis(1),
            backoff_max: Duration::from_millis(5),
            failure_limit: 5,
            cooldown: Duration::from_millis(5),
            healthy_after: Duration::from_secs(3600),
        }
    }

    /// Poll `cond` until it returns true or `timeout` elapses.
    async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        cond()
    }

    #[tokio::test]
    async fn respawns_after_panic_and_heartbeat_resumes() {
        let health = TaskHealth::new();
        let attempts = Arc::new(AtomicU32::new(0));
        let iterations = Arc::new(AtomicU64::new(0));

        let a = attempts.clone();
        let it = iterations.clone();
        supervise("panic_once", health.clone(), fast_policy(), move |state| {
            let a = a.clone();
            let it = it.clone();
            async move {
                let attempt = a.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    panic!("boom on first attempt");
                }
                // Healthy generation: beat forever.
                loop {
                    state.beat();
                    it.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            }
        });

        // Re-spawn happened (attempt 2 ran) and the healthy loop is iterating.
        assert!(
            wait_until(Duration::from_secs(2), || attempts.load(Ordering::SeqCst)
                >= 2
                && iterations.load(Ordering::SeqCst) >= 3)
            .await,
            "expected re-spawn after panic with a live, beating task"
        );

        // Heartbeat is fresh (resumed), and the breaker never opened (one transient
        // failure is well under the limit).
        let state = health.register("panic_once");
        assert!(now_ms().saturating_sub(state.heartbeat_ms()) < 1000);
        assert!(!state.breaker_open());
    }

    #[tokio::test]
    async fn breaker_opens_after_repeated_panics_without_process_exit() {
        let health = TaskHealth::new();

        supervise(
            "always_panics",
            health.clone(),
            fast_policy(),
            move |_state| async move {
                panic!("always");
            },
        );

        // After failure_limit consecutive panics the breaker opens and the task is
        // marked degraded. The process does NOT exit — this test completing proves it.
        let state = health.register("always_panics");
        assert!(
            wait_until(Duration::from_secs(2), || state.breaker_open()).await,
            "breaker should open after repeated panics"
        );

        // It keeps retrying on the cooldown cadence (does not give up): the task
        // shows up as unhealthy via the breaker for the watchdog.
        let reports = health.unhealthy(now_ms(), 60_000);
        assert!(
            reports
                .iter()
                .any(|r| r.name == "always_panics" && r.breaker_open)
        );
    }

    #[test]
    fn backoff_curve_is_capped_and_exponential() {
        let p = SupervisorPolicy::default();
        assert_eq!(backoff_for(1, &p), Duration::from_millis(50));
        assert_eq!(backoff_for(2, &p), Duration::from_millis(100));
        assert_eq!(backoff_for(3, &p), Duration::from_millis(200));
        // Capped at backoff_max (5 s) — and the shift never overflows.
        assert_eq!(backoff_for(8, &p), Duration::from_secs(5));
        assert_eq!(backoff_for(1000, &p), Duration::from_secs(5));
    }

    #[test]
    fn watchdog_flags_stale_heartbeat_but_not_a_fresh_one() {
        let health = TaskHealth::new();
        let state = health.register("wedged");
        state.beat();
        let hb = state.heartbeat_ms();

        // Fresh: not stale within the threshold.
        assert!(health.unhealthy(hb, 1000).is_empty());

        // Simulated wedge: "now" is far past the last heartbeat → stale + reported.
        let reports = health.unhealthy(hb + 5000, 1000);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].name, "wedged");
        assert!(reports[0].stale);
        assert!(!reports[0].breaker_open);
    }

    #[test]
    fn watchdog_flags_open_breaker_even_when_heartbeat_fresh() {
        let health = TaskHealth::new();
        let state = health.register("degraded");
        state.beat();
        state.set_breaker_open(true);

        let reports = health.unhealthy(state.heartbeat_ms(), 60_000);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].breaker_open);
        assert!(!reports[0].stale);
    }

    #[test]
    fn register_is_idempotent_and_shares_state() {
        let health = TaskHealth::new();
        let a = health.register("dup");
        a.set_breaker_open(true);
        let b = health.register("dup");
        assert!(
            b.breaker_open(),
            "second register returns the same shared state"
        );
        assert_eq!(health.tasks().len(), 1);
    }
}
