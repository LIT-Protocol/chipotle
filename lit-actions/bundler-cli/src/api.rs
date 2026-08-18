//! Thin HTTP client for the two lit-api-server endpoints this CLI drives:
//! `POST /core/v1/add_action` (deploy: register a bundle's CID on the network)
//! and `POST /core/v1/lit_binary_action` (run: execute a bundle by bytes or by
//! checksum). Both authenticate with the account/usage API key via `X-Api-Key`.

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;

/// A configured endpoint: base URL + API key.
pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
}

#[derive(Serialize)]
struct AddActionBody<'a> {
    action_ipfs_cid: &'a str,
    name: &'a str,
    description: &'a str,
}

#[derive(Serialize)]
struct BinaryActionBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    startup_script: Option<&'a str>,
    js_params: serde_json::Value,
}

impl Client {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            // Actions can run for minutes; give the request room.
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/core/v1/{}", self.base_url, path)
    }

    /// Register a bundle's CID on the network so this account's key may run it.
    pub fn add_action(
        &self,
        cid: &str,
        name: &str,
        description: &str,
    ) -> Result<serde_json::Value> {
        let body = AddActionBody {
            action_ipfs_cid: cid,
            name,
            description,
        };
        self.post("add_action", &body)
    }

    /// Execute a bundle, sending the bytes (first run) or referencing a cached
    /// bundle by `checksum` (repeat runs).
    pub fn lit_binary_action(
        &self,
        bundle_bytes: Option<&[u8]>,
        checksum: Option<&str>,
        startup_script: Option<&str>,
        js_params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let body = BinaryActionBody {
            bundle: bundle_bytes.map(|b| BASE64.encode(b)),
            checksum,
            startup_script,
            js_params,
        };
        self.post("lit_binary_action", &body)
    }

    fn post<B: Serialize>(&self, path: &str, body: &B) -> Result<serde_json::Value> {
        let url = self.url(path);
        let resp = self
            .http
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(body)
            .send()
            .with_context(|| format!("request to {url} failed"))?;

        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            bail!("{path} returned {status}: {}", text.trim());
        }
        // Successful responses are JSON; fall back to the raw text if not.
        Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
    }
}
