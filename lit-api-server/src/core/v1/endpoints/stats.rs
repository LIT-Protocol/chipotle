//! GET /get_system_stats — CVM memory usage and in-process cache statistics.
//!
//! Powers the monitor dapp's system dashboard (CPL-353). No guards — like
//! `/version` and `/get_lit_action_client_config`, it exposes only
//! operational metadata: byte/entry counts and socket presence, never
//! account data or secrets.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::accounts::blockchain_cache;
use crate::core::v1::health::LitActionsSocketPath;
use crate::core::v1::helpers::api_status::ApiResult;
use crate::core::v1::helpers::api_status::ErrMessage;
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::response::{CacheStats, MemoryStats, RunnerInfo, SystemStatsResponse};
use crate::stripe::StripeState;
use moka::future::Cache;
use rocket::State;
use rocket::get;
use rocket_okapi::openapi;

/// Env var overriding the gVisor runner's socket path. Same name and default
/// the `/lit_binary_action` wiring uses, so the two stay in agreement once
/// that route lands.
pub const LIT_ACTIONS_GVISOR_SOCKET_ENV: &str = "LIT_ACTIONS_GVISOR_SOCKET";

/// Default gVisor runner socket on the shared `lit-socket` volume — see
/// `docker-compose.phala.yml` and `architectureDocs/gvisor-server.md`.
pub const LIT_ACTIONS_GVISOR_SOCKET: &str = "/tmp/lit_actions_gvisor.sock";

/// Flushing moka's pending maintenance (`run_pending_tasks`) makes the counts
/// exact, but this endpoint is public — a request flood must not be able to
/// force cache maintenance in a tight loop. Flush at most once per this
/// interval; between flushes the raw counts are served (they may lag moka's
/// internal buffers slightly, which is fine for a 30s-cadence dashboard).
const FLUSH_INTERVAL_SECS: u64 = 10;

/// True when this request wins the right to flush cache maintenance —
/// at most one caller per [`FLUSH_INTERVAL_SECS`] across all requests.
fn claim_flush_slot() -> bool {
    static LAST_FLUSH_SECS: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    try_claim_flush_slot(&LAST_FLUSH_SECS, now)
}

fn try_claim_flush_slot(last_flush_secs: &AtomicU64, now: u64) -> bool {
    let last = last_flush_secs.load(Ordering::Relaxed);
    now.saturating_sub(last) >= FLUSH_INTERVAL_SECS
        && last_flush_secs
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
}

#[openapi(tag = "Configuration")]
#[get("/get_system_stats")]
pub(super) async fn get_system_stats(
    ipfs_cache: &State<Cache<String, Arc<String>>>,
    stripe_state: &State<Option<Arc<StripeState>>>,
    js_socket: &State<LitActionsSocketPath>,
) -> OpenApiResponse<SystemStatsResponse, ErrMessage> {
    let flush = claim_flush_slot();
    let mut caches = Vec::new();

    // JS action-code cache: its weigher is source length, so weighted_size
    // is the bytes of cached Lit Action source.
    if flush {
        ipfs_cache.run_pending_tasks().await;
    }
    caches.push(CacheStats {
        name: "action_code".to_string(),
        description: "JS Lit Action source cached by IPFS CID".to_string(),
        entry_count: ipfs_cache.entry_count(),
        approx_bytes: Some(ipfs_cache.weighted_size()),
    });

    if let Some(bc) = blockchain_cache::get() {
        caches.push(
            entry_stats(
                "permission_execute_action",
                "canExecuteAction results",
                bc.execute_action_cache(),
                flush,
            )
            .await,
        );
        caches.push(
            entry_stats(
                "permission_use_wallet",
                "canUseWalletInAction results",
                bc.use_wallet_cache(),
                flush,
            )
            .await,
        );
        caches.push(
            entry_stats(
                "permission_execute_and_wallet",
                "canExecuteActionAndUseWallet results",
                bc.execute_and_wallet_cache(),
                flush,
            )
            .await,
        );
        caches.push(
            entry_stats(
                "wallet_derivation",
                "getWalletDerivation results",
                bc.wallet_derivation_cache(),
                flush,
            )
            .await,
        );
        caches.push(CacheStats {
            name: "permission_generations".to_string(),
            description: "Per-account invalidation generation counters".to_string(),
            entry_count: bc.generation_count(),
            approx_bytes: None,
        });
    }

    if let Some(stripe) = stripe_state.inner() {
        for (name, entry_count) in stripe.cache_entry_counts(flush).await {
            let description = match name {
                "billing_customer" => "Stripe customer IDs by billing wallet",
                "billing_wallet" => "Billing wallet addresses by API key",
                _ => "Stripe credit balances by customer ID",
            };
            caches.push(CacheStats {
                name: name.to_string(),
                description: description.to_string(),
                entry_count,
                approx_bytes: None,
            });
        }
    }

    let gvisor_socket = std::env::var(LIT_ACTIONS_GVISOR_SOCKET_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| LIT_ACTIONS_GVISOR_SOCKET.to_string());
    let runners = vec![
        RunnerInfo {
            name: "js".to_string(),
            socket_path: js_socket.0.display().to_string(),
            socket_present: js_socket.0.exists(),
        },
        RunnerInfo {
            name: "gvisor".to_string(),
            socket_present: Path::new(&gvisor_socket).exists(),
            socket_path: gvisor_socket,
        },
    ];

    OpenApiResponse {
        response: ApiResult(Ok(SystemStatsResponse {
            memory: read_memory_stats(),
            caches,
            runners,
        }))
        .into(),
    }
}

/// Entry-count stats for a cache without a byte weigher.
async fn entry_stats<V>(
    name: &str,
    description: &str,
    cache: &Cache<String, V>,
    flush: bool,
) -> CacheStats
where
    V: Clone + Send + Sync + 'static,
{
    if flush {
        cache.run_pending_tasks().await;
    }
    CacheStats {
        name: name.to_string(),
        description: description.to_string(),
        entry_count: cache.entry_count(),
        approx_bytes: None,
    }
}

/// CVM memory from `/proc/meminfo` and this process's RSS from
/// `/proc/self/status`. Inside the CVM's containers procfs reports the
/// VM-wide figures (no lxcfs), which is exactly what the monitor wants.
/// Every field degrades to `None` where procfs is absent (macOS dev).
fn read_memory_stats() -> MemoryStats {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let total_kb = parse_kb_field(&meminfo, "MemTotal:");
    let available_kb = parse_kb_field(&meminfo, "MemAvailable:");
    MemoryStats {
        total_kb,
        available_kb,
        used_kb: total_kb.zip(available_kb).map(|(t, a)| t.saturating_sub(a)),
        process_rss_kb: parse_kb_field(&status, "VmRSS:"),
    }
}

/// Extract the numeric value of a `Label:    12345 kB` procfs line.
fn parse_kb_field(contents: &str, label: &str) -> Option<u64> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use rocket::routes;
    use std::path::PathBuf;

    #[test]
    fn parse_kb_field_reads_procfs_lines() {
        let meminfo =
            "MemTotal:       16315584 kB\nMemFree:  1064960 kB\nMemAvailable:   8377728 kB\n";
        assert_eq!(parse_kb_field(meminfo, "MemTotal:"), Some(16_315_584));
        assert_eq!(parse_kb_field(meminfo, "MemAvailable:"), Some(8_377_728));
        assert_eq!(parse_kb_field(meminfo, "VmRSS:"), None);
        assert_eq!(parse_kb_field("", "MemTotal:"), None);
    }

    #[test]
    fn flush_slot_throttles_to_one_claim_per_interval() {
        let last = AtomicU64::new(0);
        // Fresh slot (epoch 0): first caller claims, second in the same
        // second is throttled.
        assert!(try_claim_flush_slot(&last, 1_000));
        assert!(!try_claim_flush_slot(&last, 1_000));
        // Still inside the interval: throttled.
        assert!(!try_claim_flush_slot(
            &last,
            1_000 + FLUSH_INTERVAL_SECS - 1
        ));
        // Interval elapsed: claimable again, exactly once.
        assert!(try_claim_flush_slot(&last, 1_000 + FLUSH_INTERVAL_SECS));
        assert!(!try_claim_flush_slot(&last, 1_000 + FLUSH_INTERVAL_SECS));
        // Clock skew backwards never claims (saturating_sub).
        assert!(!try_claim_flush_slot(&last, 500));
    }

    #[test]
    fn get_system_stats_reports_caches_and_runners() {
        let ipfs_cache: Cache<String, Arc<String>> = Cache::builder()
            .weigher(|_k, v: &Arc<String>| v.len().try_into().unwrap_or(u32::MAX))
            .max_capacity(1024)
            .build();
        // moka's future-cache insert is async; the blocking rocket Client
        // below brings its own runtime, so seed the cache in a scratch one.
        // Flush the pending insert here so the asserted counts are exact even
        // when the endpoint's throttled flush slot was consumed elsewhere.
        rocket::tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(async {
                ipfs_cache
                    .insert("QmTest".to_string(), Arc::new("code".to_string()))
                    .await;
                ipfs_cache.run_pending_tasks().await;
            });

        let rocket = rocket::build()
            .mount("/", routes![get_system_stats])
            .manage(ipfs_cache)
            .manage(None::<Arc<StripeState>>)
            .manage(LitActionsSocketPath(PathBuf::from(
                "/nonexistent/lit_actions_test.sock",
            )));
        let client = Client::tracked(rocket).expect("valid rocket");

        let resp = client.get("/get_system_stats").dispatch();
        assert_eq!(resp.status(), Status::Ok);
        let body: SystemStatsResponse =
            serde_json::from_str(&resp.into_string().expect("body")).expect("valid JSON");

        let action_code = body
            .caches
            .iter()
            .find(|c| c.name == "action_code")
            .expect("action_code cache present");
        assert_eq!(action_code.entry_count, 1);
        assert_eq!(action_code.approx_bytes, Some(4));

        assert_eq!(body.runners.len(), 2);
        let js = &body.runners[0];
        assert_eq!(js.name, "js");
        assert!(!js.socket_present);
        assert_eq!(body.runners[1].name, "gvisor");
    }
}
