use crate::models::valid_cid;
use anyhow::{bail, Context, Result};
use moka::future::Cache;
use serde_json::{json, Value};
use sha3::{Digest, Keccak256};
use std::time::Duration;

#[derive(Clone)]
pub struct Chipotle {
    http: reqwest::Client,
    base: String,
    execution_key: String,
    master_key: String,
    keys: Cache<String, String>,
}
impl Chipotle {
    pub fn new(base: String, execution_key: String, master_key: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base,
            execution_key,
            master_key,
            keys: Cache::builder()
                .max_capacity(10000)
                .time_to_live(Duration::from_secs(300))
                .build(),
        })
    }
    /// A short, upstream-free description of why a Lit call failed. Only reqwest
    /// error classes and this module's own literal messages are ever logged;
    /// upstream bodies and action logs never are.
    fn cause(e: &anyhow::Error) -> String {
        match e.downcast_ref::<reqwest::Error>() {
            Some(e) if e.is_timeout() => "timeout".into(),
            Some(e) if e.is_connect() => "connect".into(),
            Some(e) if e.is_body() || e.is_decode() => "body".into(),
            Some(_) => "transport".into(),
            None => e.to_string(),
        }
    }
    /// Transport-level failures where the request may never have reached Lit.
    fn transient(e: &anyhow::Error) -> bool {
        e.downcast_ref::<reqwest::Error>()
            .is_some_and(|e| e.is_timeout() || e.is_connect() || e.is_request())
    }
    fn observe<T>(op: &'static str, result: Result<T>) -> Result<T> {
        if let Err(e) = &result {
            tracing::warn!(op, cause = %Self::cause(e), "lit request failed");
        }
        result
    }
    async fn body(response: reqwest::Response) -> Result<Value> {
        if !response.status().is_success() {
            bail!("Lit request failed ({})", response.status().as_u16());
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > 1024 * 1024 {
                bail!("Lit response too large");
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).context("invalid Lit response")
    }
    pub async fn public_key(&self, cid: &str) -> Result<String> {
        if !valid_cid(cid) {
            bail!("invalid CID");
        }
        if let Some(key) = self.keys.get(cid).await {
            return Ok(key);
        }
        // A key lookup is a pure read, so one retry after a transport failure
        // (connection reset, timeout) is safe and keeps a single hiccup from
        // surfacing as `lit_unavailable`.
        let params = json!({"cid":cid});
        let body = match self.execute(crate::actions::PUBLIC_KEY, &params).await {
            Err(e) if Self::transient(&e) => {
                self.execute(crate::actions::PUBLIC_KEY, &params).await?
            }
            result => result?,
        };
        let parse = || -> Result<String> {
            let key = body
                .get("public_key")
                .and_then(Value::as_str)
                .context("missing public key")?
                .to_owned();
            k256::PublicKey::from_sec1_bytes(&hex::decode(key.trim_start_matches("0x"))?)?;
            Ok(key)
        };
        let key = Self::observe("public_key", parse())?;
        self.keys.insert(cid.into(), key.clone()).await;
        Ok(key)
    }
    async fn management(&self, path: &str, body: &Value) -> Result<Value> {
        Self::observe("management", self.management_inner(path, body).await)
    }
    async fn management_inner(&self, path: &str, body: &Value) -> Result<Value> {
        let result = Self::body(
            self.http
                .post(format!("{}/core/v1/{path}", self.base))
                .header("X-Api-Key", &self.master_key)
                .header("X-Privacy-Mode", "true")
                .json(body)
                .send()
                .await?,
        )
        .await?;
        if result.get("success").and_then(Value::as_bool) != Some(true) {
            bail!("Chipotle management failed");
        }
        Ok(result)
    }
    pub async fn create_group(&self, vault: &str, cids: &[String]) -> Result<i64> {
        let hashes: Vec<String> = cids
            .iter()
            .map(|cid| format!("0x{}", hex::encode(Keccak256::digest(cid.as_bytes()))))
            .collect();
        let result=self.management("add_group",&json!({"group_name":format!("Keychain {vault}"),"group_description":"Keychain execution only","pkp_ids_permitted":[],"cid_hashes_permitted":hashes})).await?;
        let id = result["group_id"]
            .as_str()
            .context("missing group")?
            .parse::<i64>()?;
        if id <= 0 {
            bail!("invalid group");
        }
        Ok(id)
    }
    pub async fn add_action(&self, group: i64, cid: &str) -> Result<()> {
        self.management(
            "add_action_to_group",
            &json!({"group_id":group,"action_ipfs_cid":cid}),
        )
        .await?;
        Ok(())
    }
    /// Retires an action from a group. Chipotle's contract reverts when the CID is
    /// not a member, so callers must treat a failure after an ambiguous earlier
    /// success as possibly complete.
    pub async fn remove_action(&self, group: i64, cid: &str) -> Result<()> {
        let hashed = format!("0x{}", hex::encode(Keccak256::digest(cid.as_bytes())));
        self.management(
            "remove_action_from_group",
            &json!({"group_id":group,"hashed_cid":hashed}),
        )
        .await?;
        Ok(())
    }
    fn permissions(groups: &[i64]) -> Value {
        json!({"name":"Keychain user execution","description":"Execution only; owner and agent proofs remain required","can_create_groups":false,"can_delete_groups":false,"can_create_pkps":false,"manage_ipfs_ids_in_groups":[],"add_pkp_to_groups":[],"remove_pkp_from_groups":[],"execute_in_groups":groups})
    }
    pub async fn update_usage_key(&self, key: &str, groups: &[i64]) -> Result<()> {
        let mut body = Self::permissions(groups);
        body["usage_api_key"] = json!(key);
        self.management("update_usage_api_key", &body).await?;
        Ok(())
    }
    pub async fn create_usage_key(&self, groups: &[i64]) -> Result<String> {
        let result = self
            .management("add_usage_api_key", &Self::permissions(groups))
            .await?;
        let key = result["usage_api_key"]
            .as_str()
            .context("missing usage key")?;
        if key.is_empty() || key.len() > 512 {
            bail!("invalid usage key");
        }
        Ok(key.into())
    }
    pub async fn remove_usage_key(&self, key: &str) -> Result<()> {
        if self
            .management("remove_usage_api_key", &json!({"usage_api_key":key}))
            .await
            .is_ok()
        {
            return Ok(());
        }
        // A removal may commit before its response is lost. Confirm absence via
        // the master's key inventory, not account_exists (which also reports
        // false for existing accounts in chain-secured mode).
        let hash = format!("0x{}", hex::encode(Keccak256::digest(key.as_bytes())));
        for page in 0..1000 {
            let result = Self::body(
                self.http
                    .get(format!("{}/core/v1/list_api_keys", self.base))
                    .header("X-Api-Key", &self.master_key)
                    .header("X-Privacy-Mode", "true")
                    .query(&[("page_number", page), ("page_size", 100)])
                    .send()
                    .await?,
            )
            .await?;
            let keys = result.as_array().context("invalid key inventory")?;
            for entry in keys {
                let actual = entry["api_key_hash"]
                    .as_str()
                    .context("invalid key inventory")?;
                if actual.eq_ignore_ascii_case(&hash) {
                    bail!("usage key revocation unconfirmed");
                }
            }
            if keys.len() < 100 {
                return Ok(());
            }
        }
        bail!("usage key revocation unconfirmed")
    }
    pub async fn execute(&self, code: &str, params: &Value) -> Result<Value> {
        Self::observe("execute", self.execute_inner(code, params).await)
    }
    async fn execute_inner(&self, code: &str, params: &Value) -> Result<Value> {
        let body = Self::body(
            self.http
                .post(format!("{}/core/v1/lit_action", self.base))
                .header("X-Api-Key", &self.execution_key)
                .header("X-Privacy-Mode", "true")
                .json(&json!({"code": code, "js_params": params}))
                .send()
                .await?,
        )
        .await?;
        if body.get("has_error").and_then(Value::as_bool) != Some(false) {
            bail!("Lit execution failed");
        }
        let response = body.get("response").context("missing action response")?;
        // The service never returns Lit execution logs.
        Ok(response.clone())
    }
}
