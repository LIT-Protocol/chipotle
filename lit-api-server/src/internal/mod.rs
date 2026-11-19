//! Internal service-to-service primitives for the auto-top-up feature.
//!
//! Holds:
//!   • [`InternalConfig`] — env-driven config
//!     (`LIT_INTERNAL_SHARED_SECRET`) initialised once at boot.
//!   • [`InternalSecret`] — Rocket request guard verifying the
//!     `X-Internal-Secret` header on inbound calls from `lit-payments`.
//!   • [`routes::invalidate_balance_cache`] — the only internal endpoint.
//!     Called by `lit-payments` after a successful sync credit so the
//!     in-memory Stripe balance cache here drops the stale entry.
//!
//! `lit-api-server` never calls `lit-payments` in the new design (the
//! trigger is the `customer.updated` webhook on the lit-payments side), so
//! there is no outbound client helper for it. [`peers`] is the one outbound
//! call: it repeats a balance invalidation to the other instances of this
//! app, when any are configured.

pub mod config;
pub mod guard;
pub mod peers;
pub mod routes;

pub use config::InternalConfig;
pub use guard::InternalSecret;
