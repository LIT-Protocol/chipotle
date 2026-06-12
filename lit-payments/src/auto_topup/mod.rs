//! Auto top-up feature: Postgres types + queries shared by the dashboard
//! config endpoints (Phase 4) and the customer.updated webhook handler
//! (Phase 5).
//!
//! Routes are mounted from `crate::billing::*`. Webhook + reconciler lands
//! in later phases; everything that reads or writes
//! `auto_topup_config` / `auto_topup_credits` flows through this module
//! so the SQL surface stays in one place.

pub mod db;
pub mod reconciler;

#[cfg(test)]
mod reconciler_tests;
pub mod types;
pub mod webhook;
