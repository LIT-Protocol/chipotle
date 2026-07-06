//! Enterprise committed-use billing: prepaid allotment + arrears overage,
//! across a split payer/invoice Stripe-customer pair.
//!
//! See `plans/enterprise-committed-billing.md` for the full design. Entry point
//! is [`billing::spawn`], wired in `main.rs`.

pub mod billing;
pub mod calc;
pub mod db;
pub mod email;
pub mod period;
pub mod types;

pub use billing::spawn;
