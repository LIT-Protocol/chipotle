use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use strum::IntoEnumIterator;

use crate::accounts::get_node_configuration_values;

const REFRESH_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone, strum_macros::Display, strum_macros::EnumIter)]
#[allow(non_camel_case_types)]
pub enum ConfigKeys {
    REFRESH_INTERVAL_SECS,     // not implemented
    ROCKET_WORKERS_SMALL,      // not implemented
    ROCKET_WORKERS_MEDIUM,     // not implemented
    ROCKET_WORKERS_LARGE,      // not implemented
    ROCKET_FILE_SIZE_LIMIT_MB, // not implemented
    API_COST_CENTS_MANAGEMENT, // not implemented
    API_COST_CENTS_LIT_ACTION, // not implemented
    LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB,
    LIT_ACTION_DEFAULT_TIMEOUT_MS,
    LIT_ACTION_DEFAULT_ASYNC_TIMEOUT_MS,
    LIT_ACTION_DEFAULT_CLIENT_TIMEOUT_MS_BUFFER,
    LIT_ACTION_DEFAULT_MAX_CODE_LENGTH,
    LIT_ACTION_DEFAULT_MAX_CONSOLE_LOG_LENGTH,
    LIT_ACTION_DEFAULT_MAX_FETCH_COUNT,
    LIT_ACTION_DEFAULT_MAX_RESPONSE_LENGTH,
    LIT_ACTION_DEFAULT_MAX_GET_KEYS_COUNT,
    LIT_ACTION_DEFAULT_MAX_RETRIES,
}

/// Returns the display name of every `ConfigKeys` variant in declaration order.
pub fn config_key_names() -> Vec<String> {
    ConfigKeys::iter().map(|k| k.to_string()).collect()
}

pub enum ChainConfigMessage {
    Get {
        key: ConfigKeys,
        reply: flume::Sender<Option<String>>,
    },
    Update {
        values: HashMap<String, String>,
    },
}

/// A clone-cheap handle to the background chain configuration task.
#[derive(Clone)]
pub struct ChainConfig {
    tx: flume::Sender<ChainConfigMessage>,
}

impl ChainConfig {
    /// Get a configuration value by key. Returns `None` if the key is not set.
    pub async fn get(&self, key: ConfigKeys) -> Result<Option<String>> {
        let (reply_tx, reply_rx) = flume::bounded(1);
        self.tx
            .send_async(ChainConfigMessage::Get {
                key,
                reply: reply_tx,
            })
            .await
            .map_err(|e| anyhow::anyhow!("chain_config get send: {e}"))?;
        reply_rx
            .recv_async()
            .await
            .map_err(|e| anyhow::anyhow!("chain_config get recv: {e}"))
    }
}

/// Spawn the chain configuration background task and return a `ChainConfig` handle.
/// Performs an initial load from chain before returning so the first request is never empty.
pub async fn start_chain_config() -> Result<ChainConfig> {
    let (tx, rx) = flume::unbounded::<ChainConfigMessage>();

    let initial_values = load_config_from_chain().await;

    tokio::spawn(run_config_loop(initial_values, rx, tx.clone()));

    Ok(ChainConfig { tx })
}

async fn load_config_from_chain() -> HashMap<String, String> {
    match get_node_configuration_values().await {
        Ok(pairs) => {
            let map: HashMap<String, String> =
                pairs.into_iter().map(|kv| (kv.key, kv.value)).collect();
            tracing::info!("chain_config: loaded {} key(s) from chain", map.len());
            map
        }
        Err(e) => {
            tracing::error!("chain_config: failed to load from chain: {e}");
            HashMap::new()
        }
    }
}

async fn run_config_loop(
    mut values: HashMap<String, String>,
    rx: flume::Receiver<ChainConfigMessage>,
    tx: flume::Sender<ChainConfigMessage>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
    interval.tick().await; // discard the immediate first tick

    loop {
        tokio::select! {
            msg = rx.recv_async() => {
                match msg {
                    Ok(ChainConfigMessage::Get { key, reply }) => {
                        let _ = reply.send(values.get(&key.to_string()).cloned());
                    }
                    Ok(ChainConfigMessage::Update { values: new_values }) => {
                        values = new_values;
                    }
                    Err(_) => {
                        tracing::info!("chain_config: channel closed, shutting down");
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let new_values = load_config_from_chain().await;
                    let _ = tx.send_async(ChainConfigMessage::Update { values: new_values }).await;
                });
            }
        }
    }
}
