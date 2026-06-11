//! Stripe `customer.updated` webhook — the only trigger for auto top-up.
//!
//! The handler does the full §6 flow synchronously inside the webhook
//! request: HMAC verification → quick exits → mutex → fresh balance fetch
//! → PI list / failure / cap → off-session PI create → sync credit →
//! cache invalidation. Returns 5xx on transient backend errors so Stripe
//! retries; returns 200 ONLY after credit work is committed.

pub mod handler;
pub mod mutex;
pub mod sca;
pub mod signature;

#[cfg(test)]
mod handler_tests;
