//! Outbound internal-call helper.
//!
//! Phase 1 wiring for the future cache-invalidation callback to
//! `lit-api-server`: `POST {LIT_API_SERVER_BASE_URL}/internal/...` with the
//! `X-Internal-Secret` header set. The Phase 5 webhook handler will be the
//! first real caller.
//!
//! Fire-and-forget at the call site — errors are surfaced via tracing and
//! never propagated. A stale `lit-api-server` balance cache self-heals via
//! the existing 10-minute TTL, so a missed callback is a degraded path, not
//! a correctness problem.

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;

use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Build a `reqwest::Client` tuned for the internal hop:
///   • 5-second total timeout (we never want to block a webhook handler
///     waiting on the api-server)
///   • TLS verification on (the default; the production hop is TLS only)
pub fn build_client() -> Result<Client> {
    Client::builder()
        .timeout(TIMEOUT)
        .build()
        .context("building internal reqwest client")
}

/// POST a JSON body to a path on `lit-api-server` with the configured
/// `X-Internal-Secret`. The path must begin with `/`.
///
/// Returns `Ok(())` on any 2xx response; otherwise an error including the
/// status code (response body is intentionally not echoed — it may contain
/// the internal secret in a misconfigured deployment).
pub async fn post_internal<B: Serialize>(
    client: &Client,
    cfg: &Config,
    path: &str,
    body: &B,
) -> Result<()> {
    let url = format!("{}{}", cfg.lit_api_server_base_url, path);
    let resp = client
        .post(&url)
        .header("X-Internal-Secret", &cfg.lit_internal_shared_secret)
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {path}"))?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("internal POST {path} returned {status}");
    }
    Ok(())
}
