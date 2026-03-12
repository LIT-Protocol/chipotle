use crate::abstractions::transfer::chain_info::Chain;
use anyhow::Result;
use std::path::Path;
use std::{env, fs};
use toml_edit::DocumentMut;

pub struct ObservabilityConfig {
    pub log_level: String,
    #[cfg(feature = "otlp")]
    pub telemetry_endpoint: String,
}

/// Reads observability settings from the environment or NodeConfig.toml.
///
/// - `log_level`: required. Source priority: `RUST_LOG` env var → `[observability] log_level`
///   in NodeConfig.toml.
/// - `telemetry_endpoint` (otlp builds only): required. Source priority:
///   `LIT_TELEMETRY_ENDPOINT` env var → `[observability] telemetry_endpoint` in NodeConfig.toml.
///
/// Returns an error if any required value is absent from both sources.
pub fn read_observability_config() -> Result<ObservabilityConfig> {
    let env_log_level = env::var("RUST_LOG").ok();
    #[cfg(feature = "otlp")]
    let env_endpoint = env::var("LIT_TELEMETRY_ENDPOINT").ok();

    // Fast path: all required values already in env, skip file I/O.
    #[cfg(not(feature = "otlp"))]
    if let Some(ll) = env_log_level {
        return Ok(ObservabilityConfig { log_level: ll });
    }
    #[cfg(feature = "otlp")]
    if let (Some(ll), Some(ep)) = (env_log_level.as_ref(), env_endpoint.as_ref()) {
        return Ok(ObservabilityConfig {
            log_level: ll.clone(),
            telemetry_endpoint: ep.clone(),
        });
    }

    let toml_path = Path::new("NodeConfig.toml");
    let doc = if toml_path.exists() {
        fs::read_to_string(toml_path)
            .ok()
            .and_then(|s| s.parse::<DocumentMut>().ok())
    } else {
        None
    };

    let obs = doc.as_ref().and_then(|d| d.get("observability"));

    let file_log_level = obs
        .and_then(|o| o.get("log_level"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let log_level = env_log_level.or(file_log_level).ok_or_else(|| {
        anyhow::anyhow!(
            "log level not configured: set RUST_LOG or [observability] log_level in NodeConfig.toml"
        )
    })?;

    #[cfg(feature = "otlp")]
    let file_endpoint = obs
        .and_then(|o| o.get("telemetry_endpoint"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    #[cfg(feature = "otlp")]
    let telemetry_endpoint = env_endpoint.or(file_endpoint).ok_or_else(|| {
        anyhow::anyhow!(
            "telemetry endpoint not configured: set LIT_TELEMETRY_ENDPOINT or \
             [observability] telemetry_endpoint in NodeConfig.toml"
        )
    })?;

    Ok(ObservabilityConfig {
        log_level,
        #[cfg(feature = "otlp")]
        telemetry_endpoint,
    })
}

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub chain: Chain,
    pub contract_address: String,
}

use std::sync::OnceLock;

pub static GLOBAL_NODE_CONFIG: OnceLock<NodeConfig> = OnceLock::new();

pub fn init_config() -> Result<(), anyhow::Error> {
    let toml_path = Path::new("NodeConfig.toml");
    if !toml_path.exists() {
        return Err(anyhow::anyhow!(
            "NodeConfig.toml does not exist at {:?}. Current working dir is {:?}.",
            toml_path,
            env::current_dir()
        ));
    }

    let toml_contents = fs::read_to_string(toml_path).unwrap_or_default();

    if toml_contents.is_empty() {
        return Err(anyhow::anyhow!("NodeConfig.toml is empty."));
    }

    let toml_document = match toml_contents.parse::<DocumentMut>() {
        Ok(doc) => doc,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to parse NodeConfig.toml: {:?}", e));
        }
    };

    let chain: String = toml_document["chain"]["name"]
        .to_string()
        .trim()
        .replace("\"", "");
    let contract_address: String = toml_document["chain"]["contract_address"]
        .to_string()
        .trim()
        .replace("\"", "");

    let chain = match Chain::try_from_str(&chain) {
        Ok(chain) => chain,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to parse chain value in NodeConfig.toml: {:?}",
                e
            ));
        }
    };

    if contract_address.is_empty() {
        return Err(anyhow::anyhow!(
            "Contract address is empty or not found in NodeConfig.toml"
        ));
    }

    let node_config = NodeConfig {
        chain,
        contract_address,
    };

    GLOBAL_NODE_CONFIG.get_or_init(|| node_config);

    Ok(())
}
