//! On-chain restart trigger listener.
//!
//! Polls the `ServerTriggered` event from the AccountConfig contract.

use crate::accounts::signable_contract::get_read_only_account_config_contract;
use alloy::providers::Provider;
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

/// Maximum number of consecutive startup failures before giving up.
const MAX_LISTENER_RETRIES: u32 = 5;

/// Start the on-chain event listener as a background task.
pub fn start_server_trigger_listener(restart_handle: RestartHandle) {
    tokio::spawn(async move {
        let mut attempt = 0u32;
        loop {
            match run_event_listener(restart_handle.clone()).await {
                Ok(()) => break,
                Err(e) => {
                    attempt += 1;
                    if attempt >= MAX_LISTENER_RETRIES {
                        tracing::error!(
                            attempts = attempt,
                            "Server trigger listener failed permanently after {MAX_LISTENER_RETRIES} attempts: {e}"
                        );
                        break;
                    }
                    let backoff = Duration::from_secs(2u64.pow(attempt.min(5)));
                    tracing::warn!(
                        attempt,
                        backoff_secs = backoff.as_secs(),
                        "Server trigger listener failed: {e}. Retrying..."
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    });
}

async fn run_event_listener(restart_handle: RestartHandle) -> anyhow::Result<()> {
    let contract = get_read_only_account_config_contract().await?;
    let client = contract.provider();

    let start_block = client
        .get_block_number()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get initial block number: {e}"))?;

    tracing::info!(
        from_block = start_block,
        poll_interval_secs = EVENT_POLL_INTERVAL.as_secs(),
        "Server trigger event listener started"
    );

    let mut last_checked_block = start_block.saturating_sub(1);
    let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
    interval.tick().await;

    loop {
        interval.tick().await;

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

        last_checked_block = latest_block;
    }

    Ok(())
}
