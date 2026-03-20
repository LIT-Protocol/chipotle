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

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Monitors system load average + CPU pressure and exposes an overload flag.
///
/// Register as Rocket managed state via `.manage(CpuOverloadMonitor::start())`.
pub struct CpuOverloadMonitor {
    overloaded: Arc<AtomicBool>,
}

impl CpuOverloadMonitor {
    /// Starts a background tokio task that samples system load every second.
    pub fn start() -> Self {
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

        let overloaded = Arc::new(AtomicBool::new(false));

        tracing::info!(
            num_cpus = num_cpus as usize,
            load_multiplier,
            load_threshold,
            psi_threshold,
            "CPU overload monitor started (429 load shedding enabled)"
        );

        let flag = overloaded.clone();
        tokio::spawn(async move {
            let mut was_overloaded = false;
            let mut prev_psi_total: Option<u64> = None;

            loop {
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
        });

        Self { overloaded }
    }

    /// Creates a monitor with a pre-set flag (for testing from other modules).
    #[cfg(test)]
    pub fn new_with_flag(overloaded: Arc<AtomicBool>) -> Self {
        Self { overloaded }
    }

    pub fn is_overloaded(&self) -> bool {
        self.overloaded.load(Ordering::Relaxed)
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

    #[tokio::test]
    async fn read_load_avg_returns_some_on_linux() {
        let load = read_1m_load_avg().await;
        assert!(load.is_some(), "/proc/loadavg should be readable");
        assert!(load.unwrap() >= 0.0);
    }

    #[tokio::test]
    async fn read_psi_cpu_total_returns_some_on_linux() {
        let total = read_psi_cpu_total().await;
        assert!(
            total.is_some(),
            "/proc/pressure/cpu should be readable on kernels >= 4.20"
        );
    }
}
