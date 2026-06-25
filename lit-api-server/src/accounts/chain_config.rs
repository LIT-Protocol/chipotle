use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use arc_swap::ArcSwap;
use strum::IntoEnumIterator;

use crate::accounts::get_node_configuration_values;
use crate::supervisor::TaskState;

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

/// The in-memory configuration snapshot: key-name → value.
type ConfigValues = HashMap<String, String>;

/// A clone-cheap handle to the on-chain node configuration.
///
/// Reads are served from a lock-free [`ArcSwap`] **snapshot**, not from a fresh
/// fetch and not via a message channel. This is what makes the background refresh
/// task safe to re-spawn: the snapshot Arc lives in this handle, independent of the
/// refresh task's lifetime, so reads keep returning the last good values across a
/// re-spawn (rather than the empty map a failed re-init would otherwise produce).
/// A failed refresh likewise leaves the snapshot untouched.
#[derive(Clone)]
pub struct ChainConfig {
    snapshot: Arc<ArcSwap<ConfigValues>>,
}

impl ChainConfig {
    /// Get a configuration value by key. Returns `None` if the key is not set.
    ///
    /// `async`/`Result` are kept for source compatibility with callers; reads from
    /// the snapshot are infallible and synchronous under the hood.
    pub async fn get(&self, key: ConfigKeys) -> Result<Option<String>> {
        Ok(self.snapshot.load().get(&key.to_string()).cloned())
    }

    /// Get multiple configuration values in a single read.
    /// Returns a map of key-name → value for keys that are set; missing keys are omitted.
    pub async fn get_many(&self, keys: Vec<ConfigKeys>) -> Result<HashMap<String, String>> {
        let snapshot = self.snapshot.load();
        Ok(keys
            .into_iter()
            .filter_map(|k| {
                let key_str = k.to_string();
                snapshot.get(&key_str).map(|v| (key_str, v.clone()))
            })
            .collect())
    }

    /// A clone of the shared snapshot cell, for wiring the supervised refresh loop
    /// (see [`run_config_refresh_loop`]). The cell outlives any single refresh task.
    pub fn snapshot_handle(&self) -> Arc<ArcSwap<ConfigValues>> {
        self.snapshot.clone()
    }
}

/// Build the [`ChainConfig`] handle with an initial snapshot loaded from chain.
///
/// Does **not** spawn the refresh loop — wire [`run_config_refresh_loop`] into the
/// supervisor so a panic/return re-spawns it while the snapshot persists. An empty
/// snapshot is only ever produced here, on a first-boot load failure; once a good
/// snapshot exists it is retained across refreshes and re-spawns.
pub async fn start_chain_config() -> Result<ChainConfig> {
    let initial_values = load_config_from_chain().await;
    Ok(ChainConfig {
        snapshot: Arc::new(ArcSwap::from_pointee(initial_values)),
    })
}

async fn load_config_from_chain() -> ConfigValues {
    match get_node_configuration_values().await {
        Ok(pairs) => {
            let map: ConfigValues = pairs.into_iter().map(|kv| (kv.key, kv.value)).collect();
            tracing::info!("chain_config: loaded {} key(s) from chain", map.len());
            map
        }
        Err(e) => {
            tracing::error!("chain_config: failed to load from chain: {e}");
            HashMap::new()
        }
    }
}

/// Publish a refresh result to the snapshot.
///
/// `Some(values)` replaces the snapshot; `None` (a failed fetch) **retains** the
/// last good snapshot. Pulled out as a pure function so the retain-on-failure
/// invariant is unit-testable without touching the chain.
fn publish_refresh(snapshot: &ArcSwap<ConfigValues>, new_values: Option<ConfigValues>) {
    if let Some(values) = new_values {
        snapshot.store(Arc::new(values));
    }
}

/// Supervised refresh loop: every [`REFRESH_INTERVAL_SECS`] reload the config from
/// chain and publish it to `snapshot` on success. A failed fetch keeps the last
/// good snapshot.
///
/// The fetch runs **inline** in this loop — not as a per-tick child task — so a
/// re-spawn of this loop can never race a late `Update` from a previous
/// generation's orphaned child. `state.beat()` fires every iteration so a wedged
/// fetch (stuck RPC await) stops the heartbeat and the watchdog notices.
pub async fn run_config_refresh_loop(snapshot: Arc<ArcSwap<ConfigValues>>, state: Arc<TaskState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
    interval.tick().await; // discard the immediate first tick

    loop {
        interval.tick().await;
        state.beat();

        let new_values = match get_node_configuration_values().await {
            Ok(pairs) => {
                let map: ConfigValues = pairs.into_iter().map(|kv| (kv.key, kv.value)).collect();
                tracing::debug!("chain_config: refreshed {} key(s) from chain", map.len());
                Some(map)
            }
            Err(e) => {
                tracing::warn!("chain_config: refresh failed, keeping last-good snapshot: {e}");
                None
            }
        };
        publish_refresh(&snapshot, new_values);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from(pairs: &[(&str, &str)]) -> ChainConfig {
        let map: ConfigValues = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        ChainConfig {
            snapshot: Arc::new(ArcSwap::from_pointee(map)),
        }
    }

    #[tokio::test]
    async fn get_and_get_many_read_from_snapshot() {
        let cfg = config_from(&[
            ("LIT_ACTION_DEFAULT_TIMEOUT_MS", "5000"),
            ("LIT_ACTION_DEFAULT_MAX_RETRIES", "3"),
        ]);

        assert_eq!(
            cfg.get(ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS)
                .await
                .unwrap()
                .as_deref(),
            Some("5000")
        );
        // Missing key → None / omitted.
        assert!(
            cfg.get(ConfigKeys::LIT_ACTION_DEFAULT_MAX_FETCH_COUNT)
                .await
                .unwrap()
                .is_none()
        );

        let many = cfg
            .get_many(vec![
                ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
                ConfigKeys::LIT_ACTION_DEFAULT_MAX_RETRIES,
                ConfigKeys::LIT_ACTION_DEFAULT_MAX_FETCH_COUNT, // unset → omitted
            ])
            .await
            .unwrap();
        assert_eq!(many.len(), 2);
        assert_eq!(
            many.get("LIT_ACTION_DEFAULT_MAX_RETRIES")
                .map(String::as_str),
            Some("3")
        );
    }

    /// A failed refresh (codex #7 regression) must retain the last good snapshot,
    /// never clear it to an empty map.
    #[tokio::test]
    async fn failed_refresh_retains_last_good_snapshot() {
        let cfg = config_from(&[("LIT_ACTION_DEFAULT_TIMEOUT_MS", "5000")]);
        let snap = cfg.snapshot_handle();

        publish_refresh(&snap, None); // simulate a failed fetch

        assert_eq!(
            cfg.get(ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS)
                .await
                .unwrap()
                .as_deref(),
            Some("5000"),
            "failed refresh must not clear the snapshot"
        );
    }

    /// A successful refresh replaces the snapshot, and reads through the same
    /// handle observe the new values — this is exactly what survives a re-spawn,
    /// since the snapshot cell is independent of the loop task.
    #[tokio::test]
    async fn successful_refresh_updates_snapshot_observed_by_handle() {
        let cfg = config_from(&[("LIT_ACTION_DEFAULT_TIMEOUT_MS", "5000")]);
        let snap = cfg.snapshot_handle();

        let mut new_values = ConfigValues::new();
        new_values.insert(
            "LIT_ACTION_DEFAULT_TIMEOUT_MS".to_string(),
            "9000".to_string(),
        );
        publish_refresh(&snap, Some(new_values));

        assert_eq!(
            cfg.get(ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS)
                .await
                .unwrap()
                .as_deref(),
            Some("9000")
        );
    }
}
