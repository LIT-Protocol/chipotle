//! On-chain restart trigger listener.
//!
//! Polls the `ServerTriggered` event from the AccountConfig contract.

use crate::accounts::signable_contract::get_read_only_account_config_contract;
use crate::supervisor::TaskState;
use alloy::providers::Provider;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

/// Polling interval for checking new ServerTriggered events.
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// A clonable handle that can send a restart signal to the main loop.
#[derive(Clone)]
pub struct RestartHandle {
    tx: mpsc::Sender<()>,
}

impl RestartHandle {
    pub fn new(tx: mpsc::Sender<()>) -> Self {
        Self { tx }
    }

    /// Send a restart signal. Returns `true` if the signal was sent or
    /// already queued (a restart is in progress). Returns `false` only
    /// when the channel is closed.
    pub fn trigger(&self) -> bool {
        match self.tx.try_send(()) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => true,
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

/// Sentinel meaning "the watermark has never been set" — the first-ever start.
///
/// `u64::MAX` (not `0`) so a legitimately persisted watermark of block 0 — a
/// listener that first ran at genesis — is treated as a real cursor rather than
/// "unset". A block height of `u64::MAX` is not reachable in practice.
const WATERMARK_UNSET: u64 = u64::MAX;

/// Create the in-memory block watermark shared across listener re-spawns.
///
/// Held by `main` and handed to every (re)spawn of [`run_server_trigger_listener`]
/// so a re-spawn resumes from the last block it finished scanning rather than from
/// the current chain head — otherwise a `ServerTriggered` emitted while the
/// listener was down would be silently skipped (codex #6). It lives only in memory:
/// it survives task re-spawns and Rocket rebuilds, **not** a full process restart
/// (a cold start intentionally skips history).
pub fn new_block_watermark() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(WATERMARK_UNSET))
}

/// Max blocks scanned per `eth_getLogs` query. After downtime the catch-up range
/// (`watermark+1..=head`) can be arbitrarily large; chunking keeps each query under
/// typical provider range limits so a large gap can't fail every query forever and
/// wedge the cursor (which only advances on a *successful* query).
const MAX_CATCHUP_BLOCK_RANGE: u64 = 2000;

/// Inclusive upper bound of the next catch-up chunk that starts just after
/// `from_exclusive`: never past `latest`, never more than `max_range` blocks.
fn chunk_end(from_exclusive: u64, latest: u64, max_range: u64) -> u64 {
    latest.min(from_exclusive.saturating_add(max_range))
}

/// Decide the `last_checked_block` cursor for a (re)start.
///
/// First-ever start (watermark unset, i.e. `u64::MAX`) begins at `current_head - 1`:
/// we do not replay historical events on a cold boot. A re-spawn resumes from the
/// persisted watermark — including a genuine `0` — so the range `watermark+1..=head`
/// is re-scanned and any event emitted while the listener was down is still
/// delivered (the restart trigger is idempotent — a duplicate `try_send` collapses
/// to one queued restart).
fn resume_cursor(watermark: u64, current_head: u64) -> u64 {
    if watermark == WATERMARK_UNSET {
        current_head.saturating_sub(1)
    } else {
        watermark
    }
}

/// Supervised entry point for the on-chain restart listener.
///
/// The retry/backoff that the old standalone spawn did by hand (give up after 5
/// failures → silent exit) is now the supervisor's job: it re-spawns forever and
/// only marks the task degraded + alerts when the breaker opens, so on-chain
/// restart signals are never permanently ignored.
pub async fn run_server_trigger_listener(
    restart_handle: RestartHandle,
    watermark: Arc<AtomicU64>,
    state: Arc<TaskState>,
) {
    if let Err(e) = run_event_listener(restart_handle, watermark, state).await {
        // Surface the error and return; the supervisor re-spawns with backoff
        // (which, on a persistent failure, opens the breaker and alerts).
        tracing::warn!("server trigger listener exited with error: {e}");
    }
}

async fn run_event_listener(
    restart_handle: RestartHandle,
    watermark: Arc<AtomicU64>,
    state: Arc<TaskState>,
) -> anyhow::Result<()> {
    let contract = get_read_only_account_config_contract().await?;
    let client = contract.provider();

    let current_head = client
        .get_block_number()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get initial block number: {e}"))?;

    let mut last_checked_block = resume_cursor(watermark.load(Ordering::Relaxed), current_head);
    // Store immediately (in memory) so a crash before the first successful scan
    // still resumes from here rather than from a (possibly later) head.
    watermark.store(last_checked_block, Ordering::Relaxed);

    tracing::info!(
        from_block = last_checked_block,
        poll_interval_secs = EVENT_POLL_INTERVAL.as_secs(),
        "Server trigger event listener started"
    );

    let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        interval.tick().await;

        let latest_block = match client.get_block_number().await {
            Ok(b) => b,
            Err(e) => {
                // Deliberately do NOT beat here: a persistently failing RPC must
                // surface as a stale heartbeat so the watchdog alerts, instead of
                // looking healthy while silently ignoring restart signals.
                tracing::warn!("Failed to get latest block number: {e}");
                continue;
            }
        };

        if latest_block <= last_checked_block {
            // Caught up, nothing new — a healthy poll.
            state.beat();
            continue;
        }

        // Catch up in bounded chunks. A single unbounded `from..=head` query after a
        // long gap could exceed the provider's `eth_getLogs` range limit and then
        // fail on every retry, wedging the cursor (which only advances on success).
        // Each successful chunk advances + persists the watermark and beats the
        // heartbeat; a failing chunk breaks out to retry next tick from the same
        // cursor — without beating, so the watchdog sees the stall.
        while last_checked_block < latest_block {
            let from_block = last_checked_block.saturating_add(1);
            let to_block = chunk_end(last_checked_block, latest_block, MAX_CATCHUP_BLOCK_RANGE);

            match contract
                .ServerTriggered_filter()
                .from_block(from_block)
                .to_block(to_block)
                .query()
                .await
            {
                Ok(events) => {
                    if !events.is_empty() {
                        let (event, _log) = &events[events.len() - 1];
                        tracing::info!(
                            value = %event.value,
                            sender = ?event.sender,
                            event_count = events.len(),
                            block_range = format!("{from_block}..{to_block}"),
                            "ServerTriggered event detected on-chain. Sending restart signal."
                        );
                        if !restart_handle.trigger() {
                            tracing::error!("Failed to send restart signal — channel closed");
                            return Ok(());
                        }
                    }
                    // Advance + persist per chunk so a re-spawn never re-scans (or,
                    // crucially, skips) handled blocks.
                    last_checked_block = to_block;
                    watermark.store(last_checked_block, Ordering::Relaxed);
                    state.beat();
                }
                Err(e) => {
                    tracing::warn!(
                        block_range = format!("{from_block}..{to_block}"),
                        "Failed to query ServerTriggered events: {e}"
                    );
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// First-ever start begins at head-1 (no history replay).
    #[test]
    fn resume_cursor_cold_start_uses_head_minus_one() {
        assert_eq!(resume_cursor(WATERMARK_UNSET, 100), 99);
        // Genesis edge: head 0 saturates to 0.
        assert_eq!(resume_cursor(WATERMARK_UNSET, 0), 0);
    }

    /// A persisted watermark of block 0 (listener first ran at genesis) is a real
    /// cursor, NOT the "unset" sentinel: resume from 0 and re-scan 1..=head rather
    /// than jumping to head-1 and skipping early blocks (codex #6 / sentinel fix).
    #[test]
    fn resume_cursor_treats_block_zero_watermark_as_genuine() {
        assert_eq!(resume_cursor(0, 100), 0);
        assert_ne!(resume_cursor(0, 100), 99);
    }

    /// Catch-up is chunked so a large post-downtime gap can't exceed the RPC
    /// provider's range limit and wedge forever (Claude review finding 2b).
    #[test]
    fn chunk_end_bounds_catch_up_range() {
        // Large gap: capped at from + max_range.
        assert_eq!(chunk_end(50, 100_000, 2000), 2050);
        // Small gap: stops at latest.
        assert_eq!(chunk_end(50, 60, 2000), 60);
        // Exactly at the cap boundary.
        assert_eq!(chunk_end(0, 5000, 2000), 2000);
        // Saturates rather than overflowing near u64::MAX.
        assert_eq!(chunk_end(u64::MAX, u64::MAX, 2000), u64::MAX);
    }

    /// Missed-event fix (codex #6): a re-spawn resumes from the persisted
    /// watermark, so a `ServerTriggered` emitted at e.g. block 60 while the
    /// listener was down (last finished at 50, head now 100) is re-scanned in the
    /// 51..=100 range — NOT skipped by jumping to head-1 (99).
    #[test]
    fn resume_cursor_respawn_resumes_from_watermark_not_head() {
        let watermark = 50;
        let head = 100;
        let cursor = resume_cursor(watermark, head);
        assert_eq!(cursor, 50, "must resume from the persisted watermark");
        // The next scan covers cursor+1..=head, which includes block 60.
        assert!(cursor < 60 && 60 <= head);
        // The buggy behaviour would have been head-1 = 99, skipping block 60.
        assert_ne!(cursor, head - 1);
    }

    #[test]
    fn trigger_reports_open_vs_closed_channel() {
        let (tx, rx) = mpsc::channel::<()>(1);
        let handle = RestartHandle::new(tx);
        assert!(handle.trigger(), "first send succeeds");
        // Channel full (capacity 1, unread) still reports success — a restart is
        // already queued.
        assert!(handle.trigger(), "full channel collapses to queued restart");
        drop(rx);
        assert!(!handle.trigger(), "closed channel reports failure");
    }
}
