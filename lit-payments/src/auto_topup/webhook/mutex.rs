//! Per-customer in-process mutex.
//!
//! Serializes concurrent `customer.updated` deliveries for the same
//! customer so the PI-list / cap / charge logic in `handler.rs` doesn't
//! race against itself. The cache is `moka::sync` with a 5-minute TTL —
//! short-lived per the plan's §10 "this is an optimization, not a
//! correctness primitive" framing.
//!
//! Correctness still rests on:
//!   - Stripe Idempotency-Key on `paymentIntents.create`
//!   - Postgres `UNIQUE(payment_intent_id)` on `auto_topup_credits`
//!   - Stripe Idempotency-Key on `balance_transactions` write
//!
//! Those three layers hold even if the mutex is bypassed (e.g. when
//! `lit-payments` is horizontally scaled and two replicas receive copies
//! of the same event).

use std::sync::Arc;
use std::time::Duration;

use moka::sync::Cache;
use tokio::sync::Mutex;

const MUTEX_CACHE_CAPACITY: u64 = 10_000;
const MUTEX_CACHE_TTL: Duration = Duration::from_secs(300);

/// Shared mutex registry. Build once at startup and hand to Rocket state.
#[derive(Clone)]
pub struct PerCustomerMutex {
    inner: Cache<String, Arc<Mutex<()>>>,
}

impl PerCustomerMutex {
    pub fn new() -> Self {
        let inner = Cache::builder()
            .max_capacity(MUTEX_CACHE_CAPACITY)
            .time_to_idle(MUTEX_CACHE_TTL)
            .build();
        Self { inner }
    }

    /// Acquire (or create) the mutex for `customer_id`. Holders hold for
    /// the lifetime of the returned `OwnedMutexGuard`-equivalent.
    pub fn get(&self, customer_id: &str) -> Arc<Mutex<()>> {
        self.inner
            .get_with(customer_id.to_string(), || Arc::new(Mutex::new(())))
    }
}

impl Default for PerCustomerMutex {
    fn default() -> Self {
        Self::new()
    }
}
