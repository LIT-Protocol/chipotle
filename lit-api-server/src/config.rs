use crate::utils::chain_info::Chain;
use anyhow::Result;
use std::path::Path;
use std::{env, fs};
use toml_edit::DocumentMut;

pub struct ObservabilityConfig {
    pub log_level: String,
    #[cfg(feature = "otlp")]
    pub telemetry_endpoint: String,
}

/// Reads observability settings from environment variables.
///
/// - `log_level`: `RUST_LOG` env var, defaults to `"trace"`.
/// - `telemetry_endpoint` (otlp builds only): `LIT_TELEMETRY_ENDPOINT` env var, required.
///
/// Returns an error if a required value is absent.
pub fn read_observability_config() -> Result<ObservabilityConfig> {
    let log_level = env::var("RUST_LOG")
        .unwrap_or_else(|_| lit_observability::DEFAULT_LOG_FILTER.to_string());

    #[cfg(feature = "otlp")]
    let telemetry_endpoint = env::var("LIT_TELEMETRY_ENDPOINT")
        .map_err(|_| anyhow::anyhow!("LIT_TELEMETRY_ENDPOINT is not set"))?;

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
