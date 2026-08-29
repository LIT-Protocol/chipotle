//! Chipotle (`lit-api-server`) HTTP client.
//!
//! Management calls use the app's master API key; action execution uses a
//! per-tenant service usage key (encrypt) or is done by the agent itself with
//! its own usage key (reader — never through this service).

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone)]
pub struct ChipotleClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Debug)]
pub struct ChipotleError {
    pub message: String,
    pub status: Option<StatusCode>,
    pub body: Option<Value>,
}

impl std::fmt::Display for ChipotleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChipotleError {}

impl ChipotleError {
    fn transport(e: reqwest::Error) -> Self {
        // reqwest's Display hides the cause; walk the source chain so TLS/DNS/
        // connect failures are diagnosable from the API error.
        let mut message = format!("chipotle request failed: {e}");
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            message.push_str(&format!(": {s}"));
            src = s.source();
        }
        Self {
            message,
            status: None,
            body: None,
        }
    }

    /// True when Chipotle reports the entity already exists (e.g. re-registering
    /// an action CID on the account). Callers that are idempotent by design may
    /// treat this as success.
    pub fn is_already_exists(&self) -> bool {
        let text = self
            .body
            .as_ref()
            .map(|b| b.to_string().to_lowercase())
            .unwrap_or_default();
        text.contains("already") || text.contains("exists")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LitActionResponse {
    pub response: Value,
    #[serde(default)]
    pub logs: String,
    #[serde(default)]
    pub has_error: bool,
}

impl ChipotleClient {
    pub fn new(base_url: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .context("building Chipotle HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}/core/v1/{}", self.base_url, path.trim_start_matches('/'))
    }

    async fn send(
        &self,
        req: reqwest::RequestBuilder,
        api_key: &str,
        path: &str,
    ) -> Result<Value, ChipotleError> {
        let response = req
            .bearer_auth(api_key)
            .header("X-Api-Key", api_key)
            .send()
            .await
            .map_err(ChipotleError::transport)?;
        let status = response.status();
        let text = response.text().await.map_err(ChipotleError::transport)?;
        let parsed = serde_json::from_str::<Value>(&text).ok();
        if status.is_success() {
            return Ok(parsed.unwrap_or_else(|| json!({ "raw": text })));
        }
        tracing::warn!(
            target: "chipotle",
            path,
            status = %status,
            body_preview = %text.chars().take(256).collect::<String>(),
            "chipotle call failed"
        );
        Err(ChipotleError {
            message: format!(
                "chipotle {path} returned HTTP {status}: {}",
                text.chars().take(200).collect::<String>()
            ),
            status: Some(status),
            body: parsed.or_else(|| Some(json!({ "raw": text }))),
        })
    }

    async fn post_json(
        &self,
        path: &str,
        api_key: &str,
        body: &Value,
    ) -> Result<Value, ChipotleError> {
        let req = self.http.post(self.url(path)).json(body);
        self.send(req, api_key, path).await
    }

    /// Mint a vault PKP on the account. Returns its wallet address (the `pkpId`
    /// used by `Lit.Actions.Encrypt/Decrypt`).
    pub async fn create_wallet(&self, master_key: &str) -> Result<String, ChipotleError> {
        let v = self
            .post_json("create_wallet", master_key, &json!({}))
            .await?;
        string_field(&v, "wallet_address", "create_wallet")
    }

    /// Create a group permitting the given PKPs. Actions are attached separately
    /// via [`add_action_to_group`] so the server does the CID hashing.
    pub async fn add_group(
        &self,
        master_key: &str,
        name: &str,
        description: &str,
        pkp_ids: &[String],
    ) -> Result<u64, ChipotleError> {
        let v = self
            .post_json(
                "add_group",
                master_key,
                &json!({
                    "group_name": name,
                    "group_description": description,
                    "pkp_ids_permitted": pkp_ids,
                    "cid_hashes_permitted": [],
                }),
            )
            .await?;
        let raw = string_field(&v, "group_id", "add_group")?;
        parse_group_id(&raw).ok_or_else(|| ChipotleError {
            message: format!("add_group returned unparseable group_id {raw:?}"),
            status: None,
            body: Some(v),
        })
    }

    pub async fn add_action(
        &self,
        master_key: &str,
        cid: &str,
        name: &str,
        description: &str,
    ) -> Result<(), ChipotleError> {
        self.post_json(
            "add_action",
            master_key,
            &json!({ "action_ipfs_cid": cid, "name": name, "description": description }),
        )
        .await
        .map(|_| ())
    }

    pub async fn add_action_to_group(
        &self,
        master_key: &str,
        group_id: u64,
        cid: &str,
    ) -> Result<(), ChipotleError> {
        self.post_json(
            "add_action_to_group",
            master_key,
            &json!({ "group_id": group_id, "action_ipfs_cid": cid }),
        )
        .await
        .map(|_| ())
    }

    pub async fn remove_action_from_group(
        &self,
        master_key: &str,
        group_id: u64,
        hashed_cid: &str,
    ) -> Result<(), ChipotleError> {
        self.post_json(
            "remove_action_from_group",
            master_key,
            &json!({ "group_id": group_id, "hashed_cid": hashed_cid }),
        )
        .await
        .map(|_| ())
    }

    /// Mint a usage API key that can only execute actions in `groups`.
    pub async fn add_usage_api_key(
        &self,
        master_key: &str,
        name: &str,
        description: &str,
        groups: &[u64],
    ) -> Result<String, ChipotleError> {
        let v = self
            .post_json(
                "add_usage_api_key",
                master_key,
                &json!({
                    "name": name,
                    "description": description,
                    "can_create_groups": false,
                    "can_delete_groups": false,
                    "can_create_pkps": false,
                    "manage_ipfs_ids_in_groups": [],
                    "add_pkp_to_groups": [],
                    "remove_pkp_from_groups": [],
                    "execute_in_groups": groups,
                }),
            )
            .await?;
        string_field(&v, "usage_api_key", "add_usage_api_key")
    }

    pub async fn remove_usage_api_key(
        &self,
        master_key: &str,
        usage_api_key: &str,
    ) -> Result<(), ChipotleError> {
        self.post_json(
            "remove_usage_api_key",
            master_key,
            &json!({ "usage_api_key": usage_api_key }),
        )
        .await
        .map(|_| ())
    }

    /// Execute inline action code. Errors thrown by the action surface as
    /// `Err` with the action logs in the message.
    pub async fn execute_lit_action(
        &self,
        usage_api_key: &str,
        code: &str,
        js_params: Value,
    ) -> Result<LitActionResponse, ChipotleError> {
        let v = self
            .post_json(
                "lit_action",
                usage_api_key,
                &json!({ "code": code, "js_params": js_params }),
            )
            .await?;
        let parsed: LitActionResponse =
            serde_json::from_value(v.clone()).map_err(|e| ChipotleError {
                message: format!("lit_action returned an unexpected body: {e}"),
                status: None,
                body: Some(v.clone()),
            })?;
        if parsed.has_error {
            return Err(ChipotleError {
                message: format!(
                    "lit action failed: {}",
                    parsed.logs.chars().take(500).collect::<String>()
                ),
                status: None,
                body: Some(v),
            });
        }
        Ok(parsed)
    }
}

fn string_field(v: &Value, field: &str, call: &str) -> Result<String, ChipotleError> {
    v.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ChipotleError {
            message: format!("{call} response missing `{field}`"),
            status: None,
            body: Some(v.clone()),
        })
}

/// Chipotle returns group ids as strings, "decimal or hex".
pub fn parse_group_id(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// Some action responses come back as a JSON string containing JSON. Normalize.
pub fn unwrap_response(v: &Value) -> Value {
    match v {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| v.clone()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_group_ids() {
        assert_eq!(parse_group_id("42"), Some(42));
        assert_eq!(parse_group_id("0x2a"), Some(42));
        assert_eq!(parse_group_id(" 7 "), Some(7));
        assert_eq!(parse_group_id("nope"), None);
    }

    #[test]
    fn unwraps_stringified_json() {
        let v = Value::String("{\"ciphertext\":\"ab\"}".into());
        assert_eq!(unwrap_response(&v)["ciphertext"], "ab");
        let plain = json!({"a": 1});
        assert_eq!(unwrap_response(&plain), plain);
    }

    #[test]
    fn already_exists_detection() {
        let e = ChipotleError {
            message: String::new(),
            status: None,
            body: Some(json!({"error": "Action already exists"})),
        };
        assert!(e.is_already_exists());
        let e2 = ChipotleError {
            message: String::new(),
            status: None,
            body: Some(json!({"error": "insufficient balance"})),
        };
        assert!(!e2.is_already_exists());
    }
}
