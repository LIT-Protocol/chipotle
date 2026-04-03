//! On-chain restart trigger listener.
//!
//! Polls the `ServerTriggered` event from the AccountConfig contract.
//! When the diamond owner calls `serverTrigger(uint256)` on-chain, this
//! listener detects the event and sends a restart signal to the main loop
//! via [`RestartHandle`].

use crate::accounts::signable_contract::{Middleware, get_read_only_account_config_contract};
use ethers::contract::EthEvent;
use ethers::types::BlockNumber;
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

    /// Send a restart signal. Returns `true` if the signal was sent.
    pub async fn trigger(&self) -> bool {
        self.tx.send(()).await.is_ok()
    }
}

/// ABI definition for the ServerTriggered event emitted by APIConfigFacet.
/// This is defined manually to avoid depending on the full generated contract bindings
/// (which would need to be regenerated after the Solidity change).
#[derive(Clone, Debug, EthEvent)]
#[ethevent(name = "ServerTriggered", abi = "ServerTriggered(uint256,address)")]
pub struct ServerTriggeredEvent {
    pub value: ethers::types::U256,
    #[ethevent(indexed)]
    pub sender: ethers::types::Address,
}

/// Start the on-chain event listener as a background task.
///
/// Polls the AccountConfig contract for `ServerTriggered` events starting from
/// the current block. When a new event is detected, sends a restart signal
/// via the provided handle.
pub fn start_server_trigger_listener(restart_handle: RestartHandle) {
    tokio::spawn(async move {
        if let Err(e) = run_event_listener(restart_handle).await {
            tracing::error!("Server trigger listener exited with error: {e}");
        }
    });
}

async fn run_event_listener(restart_handle: RestartHandle) -> anyhow::Result<()> {
    let contract = get_read_only_account_config_contract().await?;
    let client = contract.client();

    // Start listening from the current block.
    let start_block = client
        .get_block_number()
        .await
        .map(|b| b.as_u64())
        .unwrap_or(0);

    tracing::info!(
        from_block = start_block,
        poll_interval_secs = EVENT_POLL_INTERVAL.as_secs(),
        "Server trigger event listener started"
    );

    let mut last_checked_block = start_block;
    let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
    interval.tick().await; // discard the immediate first tick

    loop {
        interval.tick().await;

        let latest_block = match client.get_block_number().await {
            Ok(b) => b.as_u64(),
            Err(e) => {
                tracing::warn!("Failed to get latest block number: {e}");
                continue;
            }
        };

        if latest_block <= last_checked_block {
            continue;
        }

        // Query ServerTriggered events in the new block range.
        let filter = contract
            .event_for_name::<ServerTriggeredEvent>("ServerTriggered")
            .map_err(|e| anyhow::anyhow!("Failed to create event filter: {e}"))?
            .from_block(BlockNumber::Number((last_checked_block + 1).into()))
            .to_block(BlockNumber::Number(latest_block.into()));

        match filter.query().await {
            Ok(events) => {
                if !events.is_empty() {
                    let event = &events[events.len() - 1]; // use the latest event
                    tracing::info!(
                        value = %event.value,
                        sender = ?event.sender,
                        event_count = events.len(),
                        block_range = format!("{}..{}", last_checked_block + 1, latest_block),
                        "ServerTriggered event detected on-chain. Sending restart signal."
                    );
                    if !restart_handle.trigger().await {
                        tracing::error!("Failed to send restart signal — channel closed");
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    block_range = format!("{}..{}", last_checked_block + 1, latest_block),
                    "Failed to query ServerTriggered events: {e}"
                );
                // Don't update last_checked_block so we retry this range.
                continue;
            }
        }

        last_checked_block = latest_block;
    }

    Ok(())
}
