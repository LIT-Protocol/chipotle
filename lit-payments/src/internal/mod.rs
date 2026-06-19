//! Internal service-to-service primitives.
//!
//! `lit-payments` initiates calls to `lit-api-server` (cache invalidation
//! after a successful sync credit — Phase 5). The reverse direction
//! (`lit-api-server` → `lit-payments`) is intentionally absent in the new
//! design: the auto-top-up trigger is the Stripe `customer.updated` webhook,
//! not a fire-and-forget call from `lit-api-server`. So we only ship the
//! outbound client helper here.

pub mod client;
