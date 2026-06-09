//! Env-driven configuration for the inbound `lit-payments` → `lit-api-server`
//! channel.
//!
//! Initialised once at boot and shoved into Rocket-managed state.
//!
//! Required env var:
//!   • `LIT_INTERNAL_SHARED_SECRET` — shared with `lit-payments`. Compared in
//!     constant time on inbound `X-Internal-Secret` headers from
//!     `lit-payments`'s sync-credit handler. `lit-api-server` does NOT call
//!     `lit-payments` in the new design (the trigger is the `customer.updated`
//!     webhook), so no outbound URL is needed here.
//!
//! Fail-closed: production builds (`#[cfg(not(any(test, debug_assertions)))]`)
//! PANIC at startup if the env var is missing or empty. Pre-fix this silently
//! returned `None`, which left the inbound `/internal/*` endpoints effectively
//! open — any caller without the (non-existent) secret was still rejected by
//! the guard, but a misconfigured deploy could route around the secret rotation
//! intent and an operator wouldn't notice until the first cache-invalidation
//! request hit the log noise. Failing the boot is the only signal that
//! reliably reaches the on-call. Test / debug builds still return `None` so
//! local `cargo run` / `cargo test` without the var set continues to work.

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct InternalConfig {
    /// Shared secret used by the [`super::guard::InternalSecret`] request
    /// guard to authorize inbound calls from `lit-payments`
    /// (`/internal/invalidate_balance_cache` after a successful sync credit).
    pub lit_internal_shared_secret: String,
}

/// Initialise from env vars.
///
/// Production builds panic if `LIT_INTERNAL_SHARED_SECRET` is missing or
/// empty (fail-closed). Test / debug builds return `None` instead so
/// developer machines without the var configured can still boot.
pub fn init() -> Option<Arc<InternalConfig>> {
    let raw = std::env::var("LIT_INTERNAL_SHARED_SECRET").ok();
    let secret = raw.as_deref().map(str::trim).unwrap_or("");
    if secret.is_empty() {
        // Production: fail loud. A missing secret means inbound
        // /internal/* would 401 every request (no key to compare
        // against) but operators wouldn't see the misconfig until the
        // first cache-invalidation tried to fire. Pre-fix this silently
        // returned None and the operator only noticed via the secondary
        // "stale balance" symptom.
        #[cfg(not(any(test, debug_assertions)))]
        panic!(
            "LIT_INTERNAL_SHARED_SECRET must be set to a non-empty value in production builds; \
             /internal/* endpoints are unconfigured"
        );
        #[cfg(any(test, debug_assertions))]
        {
            tracing::warn!(
                "LIT_INTERNAL_SHARED_SECRET missing or empty — /internal/* endpoints will reject \
                 all calls (debug build only; production would panic)"
            );
            return None;
        }
    }
    tracing::info!("internal: lit-payments inbound channel enabled");
    Some(Arc::new(InternalConfig {
        lit_internal_shared_secret: secret.to_string(),
    }))
}
