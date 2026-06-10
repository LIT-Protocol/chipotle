//! Environment configuration. Read once at startup; missing required vars fail
//! the process with a clear message — no silent defaults.
//!
//! The service is stateless: bridge state lives entirely on-chain (burns are
//! `BurnInitiated` events; completions are `usedBurnIds` / `BridgeMint` on the
//! destination), so there's nothing to persist. This config is just what the
//! UI needs to bootstrap (where the registry lives).

use anyhow::{Context, Result};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Config {
    /// Public base URL, e.g. "https://bridge.litprotocol.com".
    pub public_base_url: String,
    /// Chain id where the BridgeConfigRegistry lives (Base). Demo: 84532.
    pub registry_chain_id: i64,
    /// Deployed BridgeConfigRegistry address (0x...). None until deployed.
    pub registry_address: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            public_base_url: optional("PUBLIC_BASE_URL", "http://localhost:8000")
                .trim_end_matches('/')
                .to_string(),
            registry_chain_id: optional_i64("REGISTRY_CHAIN_ID", 84532)?,
            registry_address: optional_trimmed("REGISTRY_ADDRESS"),
        })
    }
}

fn optional(name: &str, default: &str) -> String {
    optional_trimmed(name).unwrap_or_else(|| default.to_string())
}

fn optional_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn optional_i64(name: &str, default: i64) -> Result<i64> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<i64>()
            .with_context(|| format!("env var {name} must be an integer; got {v:?}")),
        _ => Ok(default),
    }
}
