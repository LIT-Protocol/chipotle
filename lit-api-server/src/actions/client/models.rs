use std::collections::HashMap;
use std::sync::Arc;

use moka::future::Cache;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SignedData {
    pub signing_scheme: String,
    pub digest: String,
    pub pkp_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Default)]
pub struct DenoExecutionEnv {
    pub ipfs_cache: Option<Cache<String, Arc<String>>>,
    pub http_client: Option<reqwest::Client>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    pub code: String,
    pub globals: Option<serde_json::Value>,
    pub action_ipfs_id: Option<String>,
}

impl From<&str> for ExecutionOptions {
    fn from(code: &str) -> Self {
        Self {
            code: code.to_string(),
            ..Default::default()
        }
    }
}

impl From<String> for ExecutionOptions {
    fn from(code: String) -> Self {
        Self {
            code,
            ..Default::default()
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionState {
    pub response: String,
    pub logs: String,
    #[serde(skip)]
    pub fetch_count: u32,
    #[serde(skip)]
    pub sign_count: u32,
    #[serde(skip)]
    pub signed_data: HashMap<String, SignedData>,
    #[serde(skip)]
    pub claim_count: u32,
    // pub claim_data: HashMap<String, response::JsonPKPClaimKeyResponse>,
    #[serde(skip)]
    pub contract_call_count: u32,
    #[serde(skip)]
    pub broadcast_and_collect_count: u32,
    #[serde(skip)]
    pub ops_count: u32,
    /// Number of key-returning ops (GetPrivateKey, GetLitActionPrivateKey,
    /// GetLitActionPublicKey, GetLitActionWalletAddress) performed this session.
    /// Enforced against `Client::max_get_keys_count`.
    #[serde(skip)]
    pub get_keys_count: u32,
    /// Wall-clock start of execution, set before the gRPC execution request is sent.
    /// Used to derive elapsed seconds for billing instead of the unreliable gRPC `tick` field.
    #[serde(skip)]
    pub execution_start: Option<Instant>,
    /// Wall-clock end of execution, fixed by the first end-of-run flush. A
    /// later flush (a failed action is flushed twice; a failed charge is
    /// retried) settles the same duration instead of also billing the time
    /// spent settling.
    #[serde(skip)]
    pub execution_end: Option<Instant>,
    /// Tracks the last second of execution that was accounted for, for per-second charging.
    #[serde(skip)]
    pub last_billed_second: u64,
    /// Seconds of execution accumulated but not yet charged to Stripe.
    /// Flushed to Stripe every 5 seconds and at the end of execution.
    #[serde(skip)]
    pub unbilled_seconds: u64,
}

impl ExecutionState {
    /// Move the wall-clock seconds elapsed by `now` since the last accounted
    /// second into `unbilled_seconds`. Each second is counted once, whether a
    /// runtime tick or the end-of-run flush reaches it first.
    pub(crate) fn accrue_elapsed_seconds(&mut self, now: Instant) {
        // Use wall-clock time from execution_start (set before execution begins)
        // instead of the gRPC `tick` field, which reports near-zero.
        let current_second = self
            .execution_start
            .map(|start| now.saturating_duration_since(start).as_secs())
            .unwrap_or(0);
        let new_seconds = current_second.saturating_sub(self.last_billed_second);
        if new_seconds > 0 {
            self.unbilled_seconds += new_seconds;
            self.last_billed_second = current_second;
        }
    }

    /// Seconds the end-of-run flush charges, with a 1-second minimum per action.
    /// Includes time no runtime tick reported: ticks come from the runtime's
    /// event loop, so a synchronous loop sends none before the timeout kills it.
    /// The first call fixes the end of execution at `now` (see `execution_end`).
    pub(crate) fn seconds_to_flush(&mut self, now: Instant) -> u64 {
        let end = *self.execution_end.get_or_insert(now);
        self.accrue_elapsed_seconds(end);
        self.unbilled_seconds
            .max(if self.last_billed_second == 0 { 1 } else { 0 })
    }

    /// Record that the seconds from [`Self::seconds_to_flush`] are settled:
    /// charged, or written off because the balance could not cover them.
    pub(crate) fn settle_flush(&mut self) {
        self.unbilled_seconds = 0;
        // The 1-second minimum covers the run's first second. Count it as
        // accounted so a later flush in the same run doesn't charge it again:
        // a failed action is flushed when its result arrives and again on the
        // error exit.
        self.last_billed_second = self.last_billed_second.max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn started_at(start: Instant) -> ExecutionState {
        ExecutionState {
            execution_start: Some(start),
            ..Default::default()
        }
    }

    #[test]
    fn flush_bills_a_run_that_never_ticked() {
        // `while (true) {}` blocks the runtime's event loop, so no
        // UpdateResourceUsage tick arrives before the 15-minute timeout kills
        // it. All 900 seconds are billed, not the 1-second minimum.
        let start = Instant::now();
        let mut state = started_at(start);
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_secs(900)),
            900
        );
    }

    #[test]
    fn flush_bills_time_after_the_last_tick() {
        // Ticks covered the first 10 seconds (already flushed), then the code
        // entered a synchronous loop until the timeout.
        let start = Instant::now();
        let mut state = started_at(start);
        state.accrue_elapsed_seconds(start + Duration::from_secs(10));
        assert_eq!(state.unbilled_seconds, 10);
        state.unbilled_seconds = 0;
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_secs(900)),
            890
        );
    }

    #[test]
    fn each_second_is_billed_once() {
        let start = Instant::now();
        let mut state = started_at(start);
        let end = start + Duration::from_millis(42_500);
        state.accrue_elapsed_seconds(end);
        assert_eq!(state.seconds_to_flush(end), 42);
        // Charged. The error-exit path can flush again; nothing is left.
        state.settle_flush();
        assert_eq!(state.seconds_to_flush(end), 0);
    }

    #[test]
    fn minimum_is_charged_once_per_run() {
        // A failed sub-second action is flushed when its result arrives and
        // again on the error exit. Only the first flush charges the minimum.
        let start = Instant::now();
        let mut state = started_at(start);
        let end = start + Duration::from_millis(10);
        assert_eq!(state.seconds_to_flush(end), 1);
        state.settle_flush();
        assert_eq!(state.seconds_to_flush(end), 0);
    }

    #[test]
    fn later_flushes_bill_up_to_the_end_of_execution() {
        // A failed action's result arrives at 2.1 s and charging it takes
        // until 3.2 s, when the error exit flushes again. Execution ended at
        // 2.1 s, so the second flush has nothing to add.
        let start = Instant::now();
        let mut state = started_at(start);
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_millis(2_100)),
            2
        );
        state.settle_flush();
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_millis(3_200)),
            0
        );

        // A retry after a failed charge settles the same 2 seconds.
        let mut state = started_at(start);
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_millis(2_100)),
            2
        );
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_millis(3_200)),
            2
        );
    }

    #[test]
    fn short_runs_keep_the_one_second_minimum() {
        let start = Instant::now();
        let mut state = started_at(start);
        assert_eq!(
            state.seconds_to_flush(start + Duration::from_millis(300)),
            1
        );
        // No execution start (timed out before the stream opened).
        assert_eq!(ExecutionState::default().seconds_to_flush(start), 1);
    }
}
