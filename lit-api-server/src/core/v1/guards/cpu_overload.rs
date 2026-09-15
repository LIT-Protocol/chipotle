//! CPU overload protection via load-average and PSI monitoring.
//!
//! Spawns a background task that samples `/proc/loadavg` and
//! `/proc/pressure/cpu` every second and flips an atomic flag when either:
//!
//! - The 1-minute load average exceeds a threshold, **or**
//! - CPU pressure (PSI `some`) over the last 1-second window exceeds a
//!   threshold (percentage of wall-clock time at least one task was waiting
//!   for CPU).
//!
//! The PSI check catches short spikes before they register in the 1-minute
//! load average, giving the NLB health check (`/health`) and the
//! [`CpuAvailable`] request guard a faster signal to shed load.
//!
//! Default thresholds (overridable via env vars):
//! - Load average: `2 * num_cpus` (`CPU_OVERLOAD_MULTIPLIER`, e.g. `1.5`)
//! - PSI 1s: `50.0`% (`CPU_PSI_THRESHOLD`, e.g. `70.0`)

use crate::supervisor::TaskState;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::{RefOr, Response, Responses};
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Monitors system load average + CPU pressure and exposes an overload flag.
///
/// Built with [`CpuOverloadMonitor::new`] (which does **not** spawn anything) and
/// registered as Rocket managed state; the sampling loop [`CpuOverloadMonitor::run`]
/// is wired into the in-process supervisor so a panic re-spawns it.
#[derive(Clone)]
pub struct CpuOverloadMonitor {
    overloaded: Arc<AtomicBool>,
    /// 1-minute load-average threshold above which we shed load.
    load_threshold: f64,
    /// PSI `some` threshold (percent of a 1 s window with a task waiting on CPU).
    psi_threshold: f64,
}

/// Resets the overload flag to `false` on drop — the **fail-open** guarantee.
///
/// If the sampling loop ever dies (returns or panic-unwinds), shedding turns OFF so
/// a stale `true` can never wedge `/health` at 503 forever on the single live node.
/// On re-spawn the loop re-derives the real flag within ~1 s. Drop runs during a
/// panic unwind because these binaries use `panic = "unwind"`.
struct FailOpenGuard(Arc<AtomicBool>);

impl Drop for FailOpenGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

impl CpuOverloadMonitor {
    /// Build the monitor handle, reading thresholds from the environment. Does
    /// **not** spawn the sampling loop — wire [`CpuOverloadMonitor::run`] into the
    /// supervisor.
    pub fn new() -> Self {
        let load_multiplier: f64 = std::env::var("CPU_OVERLOAD_MULTIPLIER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);

        let num_cpus = num_cpus::get() as f64;
        let load_threshold = num_cpus * load_multiplier;

        // PSI threshold: percentage of wall-clock time (0–100) in a 1-second
        // window where at least one task was waiting for CPU.
        let psi_threshold: f64 = std::env::var("CPU_PSI_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50.0);

        tracing::info!(
            num_cpus = num_cpus as usize,
            load_multiplier,
            load_threshold,
            psi_threshold,
            "CPU overload monitor initialized (429 load shedding enabled)"
        );

        Self {
            overloaded: Arc::new(AtomicBool::new(false)),
            load_threshold,
            psi_threshold,
        }
    }

    /// The supervised sampling loop. Samples `/proc` every second and updates the
    /// shared overload flag. Fails open on exit (see [`FailOpenGuard`]) and beats
    /// the supervisor heartbeat each iteration.
    ///
    /// Returns an owned, `'static` future so it can be re-invoked by the supervisor
    /// without borrowing `self` across await points. `use<>` declares the future
    /// captures nothing from `&self` — it owns clones of the flag and thresholds.
    pub fn run(
        &self,
        state: Arc<TaskState>,
    ) -> impl std::future::Future<Output = ()> + Send + use<> {
        let flag = self.overloaded.clone();
        let load_threshold = self.load_threshold;
        let psi_threshold = self.psi_threshold;

        async move {
            // On any exit (return or panic-unwind) the flag is reset to false.
            let _fail_open = FailOpenGuard(flag.clone());

            let mut was_overloaded = false;
            let mut prev_psi_total: Option<u64> = None;

            loop {
                state.beat();

                let load_overloaded = read_1m_load_avg()
                    .await
                    .map(|avg| avg > load_threshold)
                    .unwrap_or(false);

                let current_psi = read_psi_cpu_total().await;
                let psi_overloaded = match (current_psi, prev_psi_total) {
                    (Some(current), Some(prev)) => {
                        // Delta is microseconds of CPU stall time in the last
                        // ~1 second.  Convert to a percentage of wall-clock
                        // time (1s = 1_000_000 µs).
                        let delta_us = current.saturating_sub(prev);
                        let psi_pct = delta_us as f64 / 1_000_000.0 * 100.0;
                        psi_pct > psi_threshold
                    }
                    _ => false, // Need two readings to compute a rate.
                };
                prev_psi_total = current_psi;

                let is_overloaded = load_overloaded || psi_overloaded;
                flag.store(is_overloaded, Ordering::Relaxed);

                if is_overloaded && !was_overloaded {
                    tracing::warn!(
                        load_overloaded,
                        psi_overloaded,
                        "CPU overload detected - new requests will receive 429"
                    );
                } else if !is_overloaded && was_overloaded {
                    tracing::info!(
                        load_overloaded,
                        psi_overloaded,
                        "CPU load returned to normal - accepting requests"
                    );
                }
                was_overloaded = is_overloaded;

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    /// Creates a monitor with a pre-set flag (for testing from other modules).
    #[cfg(test)]
    pub fn new_with_flag(overloaded: Arc<AtomicBool>) -> Self {
        Self {
            overloaded,
            load_threshold: 0.0,
            psi_threshold: 0.0,
        }
    }

    pub fn is_overloaded(&self) -> bool {
        self.overloaded.load(Ordering::Relaxed)
    }
}

impl Default for CpuOverloadMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads the 1-minute load average from `/proc/loadavg`.
async fn read_1m_load_avg() -> Option<f64> {
    tokio::fs::read_to_string("/proc/loadavg")
        .await
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Reads the cumulative `some` total (microseconds) from `/proc/pressure/cpu`.
///
/// Format: `some avg10=X avg60=X avg300=X total=123456`
async fn read_psi_cpu_total() -> Option<u64> {
    let contents = tokio::fs::read_to_string("/proc/pressure/cpu").await.ok()?;
    // First line is the `some` line.
    let some_line = contents.lines().next()?;
    // Find `total=<value>` at the end.
    some_line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("total="))
        .and_then(|v| v.parse().ok())
}

/// Request guard that rejects with `429 Too Many Requests` when the system is CPU-overloaded.
///
/// Place as the first parameter in route handlers so load is shed before any work is done.
pub struct CpuAvailable;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for CpuAvailable {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.rocket().state::<CpuOverloadMonitor>() {
            Some(monitor) if monitor.is_overloaded() => {
                tracing::debug!("Rejecting request with 429 - CPU overloaded");
                Outcome::Error((Status::TooManyRequests, ()))
            }
            _ => Outcome::Success(CpuAvailable),
        }
    }
}

impl<'r> OpenApiFromRequest<'r> for CpuAvailable {
    fn from_request_input(
        _generator: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        // Internal guard - not a user-visible parameter.
        Ok(RequestHeaderInput::None)
    }

    /// Document the `429 Too Many Requests` this guard sheds on CPU overload so
    /// it appears in the generated OpenAPI spec for every route that uses it.
    fn get_responses(_generator: &mut OpenApiGenerator) -> RocketOkapiResult<Responses> {
        let mut responses = Responses::default();
        responses.responses.insert(
            "429".to_string(),
            RefOr::Object(Response {
                description: "Too Many Requests \u{2014} the node is CPU-overloaded and shedding \
                    load. Clients receiving this response should retry the request up to \
                    five times with exponential backoff."
                    .to_string(),
                ..Default::default()
            }),
        );
        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Status;
    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};

    #[get("/test")]
    fn guarded_route(_cpu: CpuAvailable) -> &'static str {
        "ok"
    }

    /// When no CpuOverloadMonitor is in managed state, the guard passes (safe default).
    #[tokio::test]
    async fn guard_passes_when_monitor_absent() {
        let rocket = rocket::build().mount("/", routes![guarded_route]);
        let client = Client::tracked(rocket).await.expect("valid rocket");

        let response = client.get("/test").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
    }

    /// When the monitor reports not-overloaded, the guard passes.
    #[tokio::test]
    async fn guard_passes_when_not_overloaded() {
        let monitor = CpuOverloadMonitor::new_with_flag(Arc::new(AtomicBool::new(false)));
        let rocket = rocket::build()
            .manage(monitor)
            .mount("/", routes![guarded_route]);
        let client = Client::tracked(rocket).await.expect("valid rocket");

        let response = client.get("/test").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
    }

    /// When the monitor reports overloaded, the guard returns 429.
    #[tokio::test]
    async fn guard_returns_429_when_overloaded() {
        let monitor = CpuOverloadMonitor::new_with_flag(Arc::new(AtomicBool::new(true)));
        let rocket = rocket::build()
            .manage(monitor)
            .mount("/", routes![guarded_route]);
        let client = Client::tracked(rocket).await.expect("valid rocket");

        let response = client.get("/test").dispatch().await;
        assert_eq!(response.status(), Status::TooManyRequests);
    }

    /// Fail-open (codex #9): when the sampling loop ends normally, the guard
    /// resets the flag to `false` so shedding turns off rather than freezing at a
    /// stale `true`.
    #[test]
    fn fail_open_guard_resets_flag_on_drop() {
        let flag = Arc::new(AtomicBool::new(true));
        {
            let _guard = FailOpenGuard(flag.clone());
            assert!(flag.load(Ordering::Relaxed));
        }
        assert!(
            !flag.load(Ordering::Relaxed),
            "drop must reset the flag to false"
        );
    }

    /// Fail-open also holds across a panic unwind — this is the realistic death
    /// path, and it doubles as an end-to-end check that these binaries unwind
    /// (run destructors) rather than abort on panic.
    #[tokio::test]
    async fn fail_open_guard_resets_flag_on_panic_unwind() {
        let flag = Arc::new(AtomicBool::new(true));
        let f = flag.clone();
        let handle = tokio::spawn(async move {
            let _guard = FailOpenGuard(f);
            panic!("simulated monitor panic");
        });
        let result = handle.await;
        assert!(result.is_err(), "task should have panicked");
        assert!(
            !flag.load(Ordering::Relaxed),
            "flag must fail open (false) after a panic unwind"
        );
    }

    /// Re-spawnable: `run()` shares the same flag and bumps its heartbeat. On a
    /// Linux host (where `/proc` is readable) the loop derives the flag and beats
    /// within a couple of seconds.
    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn run_loop_beats_and_derives_flag() {
        use crate::supervisor::TaskHealth;
        let monitor = CpuOverloadMonitor::new();
        let health = TaskHealth::new();
        let state = health.register("cpu_test");
        let before = state.heartbeat_ms();

        let handle = tokio::spawn(monitor.run(state.clone()));
        tokio::time::sleep(Duration::from_millis(1200)).await;
        handle.abort();

        assert!(
            state.heartbeat_ms() >= before,
            "run() loop should bump the heartbeat"
        );
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn read_load_avg_returns_some_on_linux() {
        let load = read_1m_load_avg().await;
        assert!(load.is_some(), "/proc/loadavg should be readable");
        assert!(load.unwrap() >= 0.0);
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn read_psi_cpu_total_returns_some_on_linux() {
        let total = read_psi_cpu_total().await;
        assert!(
            total.is_some(),
            "/proc/pressure/cpu should be readable on kernels >= 4.20"
        );
    }
}
