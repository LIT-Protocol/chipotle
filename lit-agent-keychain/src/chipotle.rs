use crate::models::valid_cid;
use anyhow::{bail, Context, Result};
use moka::future::Cache;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct Chipotle {
    http: reqwest::Client,
    base: String,
    execution_key: String,
    keys: Cache<String, String>,
}
impl Chipotle {
    pub fn new(base: String, execution_key: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base,
            execution_key,
            keys: Cache::builder()
                .max_capacity(10000)
                .time_to_live(Duration::from_secs(300))
                .build(),
        })
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
        let body = Self::body(
            self.http
                .get(format!("{}/core/v1/lit_action_public_key/{cid}", self.base))
                .send()
                .await?,
        )
        .await?;
        let key = body
            .get("public_key")
            .and_then(Value::as_str)
            .context("missing public key")?
            .to_owned();
        k256::PublicKey::from_sec1_bytes(&hex::decode(key.trim_start_matches("0x"))?)?;
        self.keys.insert(cid.into(), key.clone()).await;
        Ok(key)
    }
    pub async fn execute(&self, code: &str, params: &Value) -> Result<Value> {
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
