//! Shared Stripe primitives for Lit billing services.
//!
//! This crate owns the customer-identity invariant: every Stripe customer is
//! keyed by `metadata.wallet_address`. Both `lit-api-server` (TEE) and
//! `lit-payments` (non-TEE) depend on this crate so the two services cannot
//! drift on how customers are identified or how balances are read.
//!
//! Scope:
//! - [`StripeClient`] — credentials + HTTP plumbing, no caching.
//! - [`customer`] — wallet ↔ Stripe customer lookup, email updates.
//! - [`balance`] — credit-balance reads.
//! - [`reporting`] — pagination helpers + per-day aggregation for the
//!   `stripe_report` binary.
//! - [`format`] — pure helpers (`cents_to_display`, `unix_to_utc_date`).
//!
//! Caching, charge flows, and PaymentIntent flows are intentionally NOT here —
//! they live in `lit-api-server` because they depend on its in-process caches
//! and on-chain wallet resolution.

pub mod balance;
pub mod billing_auth;
pub mod client;
pub mod customer;
pub mod eip712;
pub mod format;
pub mod http;
pub mod on_chain;
pub mod reporting;

pub use client::StripeClient;
pub use http::{StripeError, StripeResponse};
