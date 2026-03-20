//! CPU overload protection via load-average monitoring.
//!
//! Spawns a background task that samples `/proc/loadavg` every second and flips
//! an atomic flag when the 1-minute load average exceeds a threshold.  The
//! [`CpuAvailable`] request guard checks this flag and rejects with
//! `429 Too Many Requests` when the system is overloaded, preventing latency
//! degradation for requests that *are* admitted.
//!
//! Default threshold: `2 * num_cpus` (overridable via `CPU_OVERLOAD_MULTIPLIER`
//! env var, e.g. `CPU_OVERLOAD_MULTIPLIER=1.5`).

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Monitors system load average and exposes an overload flag.
///
/// Register as Rocket managed state via `.manage(CpuOverloadMonitor::start())`.
pub struct CpuOverloadMonitor {
    overloaded: Arc<AtomicBool>,
}

impl CpuOverloadMonitor {
    /// Starts a background tokio task that samples `/proc/loadavg` every second.
    pub fn start() -> Self {
        let multiplier: f64 = std::env::var("CPU_OVERLOAD_MULTIPLIER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);

        let num_cpus = num_cpus::get() as f64;
        let threshold = num_cpus * multiplier;
        let overloaded = Arc::new(AtomicBool::new(false));

        tracing::info!(
            num_cpus = num_cpus as usize,
            multiplier,
            threshold,
            "CPU overload monitor started (429 load shedding enabled)"
        );

        let flag = overloaded.clone();
        tokio::spawn(async move {
            let mut was_overloaded = false;
            loop {
                if let Some(load_avg) = read_1m_load_avg().await {
                    let is_overloaded = load_avg > threshold;
                    flag.store(is_overloaded, Ordering::Relaxed);

                    if is_overloaded && !was_overloaded {
                        tracing::warn!(
                            load_avg,
                            threshold,
                            "CPU overload detected - new requests will receive 429"
                        );
                    } else if !is_overloaded && was_overloaded {
                        tracing::info!(
                            load_avg,
                            threshold,
                            "CPU load returned to normal - accepting requests"
                        );
                    }
                    was_overloaded = is_overloaded;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

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
        let monitor = CpuOverloadMonitor {
            overloaded: Arc::new(AtomicBool::new(false)),
        };
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
        let monitor = CpuOverloadMonitor {
            overloaded: Arc::new(AtomicBool::new(true)),
        };
        let rocket = rocket::build()
            .manage(monitor)
            .mount("/", routes![guarded_route]);
        let client = Client::tracked(rocket).await.expect("valid rocket");

        let response = client.get("/test").dispatch().await;
        assert_eq!(response.status(), Status::TooManyRequests);
    }

    #[tokio::test]
    async fn read_load_avg_returns_some_on_linux() {
        // /proc/loadavg should always be readable on Linux.
        let load = read_1m_load_avg().await;
        assert!(load.is_some(), "/proc/loadavg should be readable");
        assert!(load.unwrap() >= 0.0);
    }
}
