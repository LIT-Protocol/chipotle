use crate::abstractions::transfer::chain_info::Chain;
use anyhow::Result;
use std::path::Path;
use std::{env, fs};
use toml_edit::DocumentMut;

pub const DEFAULT_LOG_LEVEL: &str = "info";
pub const DEFAULT_TELEMETRY_ENDPOINT: &str = "http://127.0.0.1:4317";

pub struct ObservabilityConfig {
    pub log_level: String,
    pub telemetry_endpoint: String,
}

/// Reads observability settings from NodeConfig.toml if present, otherwise returns defaults.
/// These can be overridden at runtime via `RUST_LOG` (log level) and
/// `LIT_TELEMETRY_ENDPOINT` (collector endpoint).
pub fn read_observability_config() -> ObservabilityConfig {
    let log_level = env::var("RUST_LOG").ok();
    let telemetry_endpoint = env::var("LIT_TELEMETRY_ENDPOINT").ok();

    // Fast path: both values from env, no file read needed.
    if let (Some(ll), Some(ep)) = (log_level.as_ref(), telemetry_endpoint.as_ref()) {
        return ObservabilityConfig { log_level: ll.clone(), telemetry_endpoint: ep.clone() };
    }

    let toml_path = Path::new("NodeConfig.toml");
    let doc = if toml_path.exists() {
        fs::read_to_string(toml_path)
            .ok()
            .and_then(|s| s.parse::<DocumentMut>().ok())
    } else {
        None
    };

    let file_log_level = doc.as_ref()
        .and_then(|d| d.get("observability"))
        .and_then(|o| o.get("log_level"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let file_endpoint = doc.as_ref()
        .and_then(|d| d.get("observability"))
        .and_then(|o| o.get("telemetry_endpoint"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    ObservabilityConfig {
        log_level: log_level
            .or(file_log_level)
            .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string()),
        telemetry_endpoint: telemetry_endpoint
            .or(file_endpoint)
            .unwrap_or_else(|| DEFAULT_TELEMETRY_ENDPOINT.to_string()),
    }
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
