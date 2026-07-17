//! The local execution job: the developer-authored stand-in for what
//! lit-api-server hands the sandbox. Mirrors the gvisor-server `Job`
//! envelope and the `lit job` JSON shape from PR #557.

use std::path::Path;

use anyhow::{Context as _, Result};
use serde::Deserialize;
use serde_json::{Value, json};

/// Same default execution timeout as the runner (15 minutes).
const DEFAULT_TIMEOUT_MS: u64 = 1000 * 60 * 15;
/// Fallback content id when none is supplied (env / job file / flag).
pub const DEFAULT_IPFS_ID: &str = "local-action";

/// A job file (`lit.job.json` by convention). Every field is optional so an
/// empty `{}` — or no file at all — is a valid job.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawJob {
    ipfs_id: Option<String>,
    timeout_ms: Option<u64>,
    http_headers: std::collections::BTreeMap<String, String>,
    js_params: Value,
    auth_context: Value,
}

#[derive(Debug)]
pub struct Job {
    pub ipfs_id: String,
    pub timeout_ms: u64,
    pub http_headers: std::collections::BTreeMap<String, String>,
    pub js_params: Value,
    pub auth_context: Value,
}

impl Job {
    /// Load a job, applying defaults. `default_ipfs_id` is used when the job
    /// file omits one (itself resolved from `--ipfs-id` / env upstream).
    pub fn load(path: Option<&Path>, default_ipfs_id: &str) -> Result<Self> {
        let raw = match path {
            Some(p) => {
                let bytes = std::fs::read(p)
                    .with_context(|| format!("failed to read job file {}", p.display()))?;
                serde_json::from_slice::<RawJob>(&bytes)
                    .with_context(|| format!("job file {} is not valid JSON", p.display()))?
            }
            None => RawJob::default(),
        };

        Ok(Self {
            ipfs_id: raw.ipfs_id.unwrap_or_else(|| default_ipfs_id.to_string()),
            timeout_ms: raw.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
            http_headers: raw.http_headers,
            js_params: raw.js_params,
            auth_context: raw.auth_context,
        })
    }

    /// The `lit job` payload, matching the guest CLI's key names.
    pub fn to_json(&self) -> Value {
        json!({
            "ipfsId": self.ipfs_id,
            "timeoutMs": self.timeout_ms,
            "httpHeaders": self.http_headers,
            "jsParams": self.js_params,
            "authContext": self.auth_context,
        })
    }
}
