//! Environment configuration. Read once at startup; missing required vars fail
//! the process with a clear message — no silent defaults.
//!
//! The service is stateless: bridge state lives entirely on-chain (burns are
//! `BurnInitiated` events; completions are `usedBurnIds` / `BridgeMint` on the
//! destination), so there's nothing to persist. This config is just what the
//! bridging UI needs to bootstrap (where the registry lives + the chains/token
//! it can bridge). It's served verbatim at `GET /api/config`.

use anyhow::{Context, Result};

/// A bridgeable chain + the token's address on it, surfaced to the UI. The UI
/// reads/writes chain via `rpc` (a public endpoint) and the user's wallet.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChainInfo {
    pub chain_id: i64,
    pub name: String,
    pub rpc: String,
    pub token: String,
    #[serde(default)]
    pub explorer: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Config {
    /// Public base URL, e.g. "https://bridge.litprotocol.com".
    pub public_base_url: String,
    /// Chain id where the BridgeConfigRegistry lives (Base). Demo: 84532.
    pub registry_chain_id: i64,
    /// Deployed BridgeConfigRegistry address (0x...). None until deployed.
    pub registry_address: Option<String>,
    /// Token ticker shown in the UI (e.g. "BRDG").
    pub token_symbol: Option<String>,
    /// Fee in basis points (matches the on-chain BridgeToken fee), shown in the UI.
    pub fee_bps: i64,
    /// Bridgeable chains + token addresses (from CHAINS_JSON). Empty disables the UI's bridge form.
    pub chains: Vec<ChainInfo>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            public_base_url: optional("PUBLIC_BASE_URL", "http://localhost:8000")
                .trim_end_matches('/')
                .to_string(),
            registry_chain_id: optional_i64("REGISTRY_CHAIN_ID", 84532)?,
            registry_address: optional_trimmed("REGISTRY_ADDRESS"),
            token_symbol: optional_trimmed("TOKEN_SYMBOL"),
            fee_bps: optional_i64("FEE_BPS", 10)?,
            chains: parse_chains()?,
        })
    }
}

/// Parse `CHAINS_JSON` (a JSON array of ChainInfo) for the UI. Empty/unset → no
/// chains (the UI shows a "not configured" state rather than crashing).
fn parse_chains() -> Result<Vec<ChainInfo>> {
    match optional_trimmed("CHAINS_JSON") {
        None => Ok(Vec::new()),
        Some(s) => serde_json::from_str(&s)
            .context("CHAINS_JSON must be a JSON array of {chain_id,name,rpc,token,explorer}"),
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
