//! Env-driven configuration for the inbound `lit-payments` → `lit-api-server`
//! channel.
//!
//! Initialised once at boot and shoved into Rocket-managed state. Mirrors the
//! `stripe::init()` pattern: returns `None` when the env var is missing so
//! that local dev / TEE simulator runs without auto-top-up wiring keep
//! working.
//!
//! Required env var:
//!   • `LIT_INTERNAL_SHARED_SECRET` — shared with `lit-payments`. Compared in
//!     constant time on inbound `X-Internal-Secret` headers from
//!     `lit-payments`'s sync-credit handler. `lit-api-server` does NOT call
//!     `lit-payments` in the new design (the trigger is the `customer.updated`
//!     webhook), so no outbound URL is needed here.

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct InternalConfig {
    /// Shared secret used by the [`super::guard::InternalSecret`] request
    /// guard to authorize inbound calls from `lit-payments`
    /// (`/internal/invalidate_balance_cache` after a successful sync credit).
    pub lit_internal_shared_secret: String,
}

/// Initialise from env vars. Returns `None` when the secret is missing or
/// empty — callers should mirror the existing `stripe::init()` pattern and
/// route around the absence.
pub fn init() -> Option<Arc<InternalConfig>> {
    let secret = std::env::var("LIT_INTERNAL_SHARED_SECRET").ok()?;
    if secret.trim().is_empty() {
        return None;
    }
    tracing::info!("internal: lit-payments inbound channel enabled");
    Some(Arc::new(InternalConfig {
        lit_internal_shared_secret: secret,
    }))
}
