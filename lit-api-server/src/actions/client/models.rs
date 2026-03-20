use std::collections::HashMap;

use moka::future::Cache;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SignedData {
    pub signing_scheme: String,
    pub digest: String,
    pub pkp_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Default)]
pub struct DenoExecutionEnv {
    pub ipfs_cache: Option<Cache<String, String>>,
    pub http_client: Option<reqwest::Client>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    pub code: String,
    pub globals: Option<serde_json::Value>,
    pub action_ipfs_id: Option<String>,
}

impl From<&str> for ExecutionOptions {
    fn from(code: &str) -> Self {
        Self {
            code: code.to_string(),
            ..Default::default()
        }
    }
}

impl From<String> for ExecutionOptions {
    fn from(code: String) -> Self {
        Self {
            code,
            ..Default::default()
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionState {
    pub response: String,
    pub logs: String,
    #[serde(skip)]
    pub fetch_count: u32,
    #[serde(skip)]
    pub sign_count: u32,
    #[serde(skip)]
    pub signed_data: HashMap<String, SignedData>,
    #[serde(skip)]
    pub claim_count: u32,
    // pub claim_data: HashMap<String, response::JsonPKPClaimKeyResponse>,
    #[serde(skip)]
    pub contract_call_count: u32,
    #[serde(skip)]
    pub broadcast_and_collect_count: u32,
    #[serde(skip)]
    pub ops_count: u32,
    #[serde(skip)]
    pub wallet_permission_cache: HashMap<String, bool>,
}
