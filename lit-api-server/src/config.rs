use crate::abstractions::transfer::chain_info::Chain;
use anyhow::Result;
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub chain: Chain,
    pub secret: String,
    pub contract_address: String,
}

use std::sync::OnceLock;

pub static GLOBAL_NODE_CONFIG: OnceLock<NodeConfig> = OnceLock::new();

pub fn init_config() -> Result<(), anyhow::Error> {
    let toml_path = Path::new("NodeConfig.toml");
    let toml_contents = fs::read_to_string(toml_path).unwrap_or_default();

    if toml_contents.is_empty() {
        return Err(anyhow::anyhow!("No config found in NodeConfig.toml"));
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
    let secret: String = toml_document["chain"]["secret"]
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

    if secret.is_empty() {
        return Err(anyhow::anyhow!(
            "Secret is empty or not found in NodeConfig.toml"
        ));
    }

    let node_config = NodeConfig {
        chain: chain.clone(),
        secret: secret,
        contract_address: contract_address,
    };

    GLOBAL_NODE_CONFIG.get_or_init(|| node_config);

    return Ok(());
}
