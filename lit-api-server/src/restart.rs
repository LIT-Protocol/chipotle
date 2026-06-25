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
const WATERMARK_UNSET: u64 = 0;

/// Create the persisted block watermark shared across listener re-spawns.
///
/// Held by `main` and handed to every (re)spawn of [`run_server_trigger_listener`]
/// so a re-spawn resumes from the last block it finished scanning rather than from
/// the current chain head — otherwise a `ServerTriggered` emitted while the
/// listener was down would be silently skipped (codex #6).
pub fn new_block_watermark() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(WATERMARK_UNSET))
}

/// Decide the `last_checked_block` cursor for a (re)start.
///
/// First-ever start (watermark unset) begins at `current_head - 1`: we do not
/// replay historical events on a cold boot. A re-spawn resumes from the persisted
/// watermark, so the range `watermark+1..=head` is re-scanned and any event emitted
/// while the listener was down is still delivered (the restart trigger is
/// idempotent — a duplicate `try_send` collapses to one queued restart).
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
    // Persist immediately so a crash before the first successful scan still resumes
    // from here rather than from a (possibly later) head.
    watermark.store(last_checked_block, Ordering::Relaxed);

    tracing::info!(
        from_block = last_checked_block,
        poll_interval_secs = EVENT_POLL_INTERVAL.as_secs(),
        "Server trigger event listener started"
    );

    let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
    interval.tick().await;

    loop {
        interval.tick().await;
        state.beat();

        let latest_block = match client.get_block_number().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to get latest block number: {e}");
                continue;
            }
        };

        if latest_block <= last_checked_block {
            continue;
        }

        let events = contract
            .ServerTriggered_filter()
            .from_block(last_checked_block.saturating_add(1))
            .to_block(latest_block)
            .query()
            .await;

        match events {
            Ok(events) => {
                if !events.is_empty() {
                    let (event, _log) = &events[events.len() - 1];
                    tracing::info!(
                        value = %event.value,
                        sender = ?event.sender,
                        event_count = events.len(),
                        block_range = format!("{}..{}", last_checked_block.saturating_add(1), latest_block),
                        "ServerTriggered event detected on-chain. Sending restart signal."
                    );
                    if !restart_handle.trigger() {
                        tracing::error!("Failed to send restart signal — channel closed");
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    block_range =
                        format!("{}..{}", last_checked_block.saturating_add(1), latest_block),
                    "Failed to query ServerTriggered events: {e}"
                );
                continue;
            }
        }

        // Advance the cursor only after the range has been handled (triggered on),
        // and persist it so a re-spawn never re-scans (or, crucially, skips) it.
        last_checked_block = latest_block;
        watermark.store(last_checked_block, Ordering::Relaxed);
    }

    Ok(())
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
