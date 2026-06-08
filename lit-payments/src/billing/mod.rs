//! Dashboard-facing billing endpoints (mounted under `/billing/`).
//!
//! All routes here sit behind the [`BillingAuth`] guard from
//! `lit-billing-auth`, which authenticates wallet-sig (preferred) or
//! API-key callers identically. lit-payments delegates verification to
//! lit-api-server's internal endpoints over the existing
//! `X-Internal-Secret` channel (see `auth_resolver::HttpAuthResolver`).

pub mod auto_topup_config;
pub mod sca_resume;
pub mod setup_intent;

#[cfg(test)]
mod auto_topup_config_tests;
#[cfg(test)]
mod sca_resume_tests;
#[cfg(test)]
pub(crate) mod setup_intent_tests;
