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
    // No redirects: the shared secret travels in a custom header, which a
    // redirect to another origin would carry along.
    Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("building internal reqwest client")
}

/// Every `lit-api-server` instance the callback must reach: the primary base
/// URL first, then any extra instances, without duplicates.
pub fn target_base_urls(cfg: &Config) -> Vec<&str> {
    let mut urls: Vec<&str> = vec![cfg.lit_api_server_base_url.as_str()];
    for url in &cfg.lit_api_server_extra_base_urls {
        if !urls.contains(&url.as_str()) {
            urls.push(url);
        }
    }
    urls
}

/// The host of a base URL, for logs. Never the whole URL: a misconfigured one
/// may carry a credential.
fn host_of(base: &str) -> &str {
    let rest = base.split_once("://").map_or(base, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

/// POST a JSON body to a path on every configured `lit-api-server` instance
/// with the configured `X-Internal-Secret`. The path must begin with `/`.
///
/// Each instance keeps its own cache, so every one is tried even after a
/// failure. Returns `Ok(())` only if all answered 2xx; otherwise an error
/// naming the instances that did not (response bodies are intentionally not
/// echoed — they may contain the internal secret in a misconfigured
/// deployment).
pub async fn post_internal<B: Serialize>(
    client: &Client,
    cfg: &Config,
    path: &str,
    body: &B,
) -> Result<()> {
    let mut failures = Vec::new();
    for base in target_base_urls(cfg) {
        let outcome = client
            .post(format!("{base}{path}"))
            .header("X-Internal-Secret", &cfg.lit_internal_shared_secret)
            .json(body)
            .send()
            .await;
        match outcome {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => failures.push(format!("{} returned {}", host_of(base), resp.status())),
            Err(e) => failures.push(format!("{}: {}", host_of(base), e.without_url())),
        }
    }
    if !failures.is_empty() {
        anyhow::bail!("internal POST {path} failed: {}", failures.join("; "));
    }
    Ok(())
}
