use crate::config::Config;
use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

pub const VERSION: &str = "2025-03-31.basil";
#[derive(Clone)]
pub struct Stripe {
    http: reqwest::Client,
    base: String,
    secret: String,
    pub price: String,
    pub portal_configuration: String,
    webhook_secret: String,
}
impl Stripe {
    pub fn new(cfg: &Config) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base: cfg.stripe_api_url.clone(),
            secret: cfg.stripe_secret_key.clone(),
            price: cfg.stripe_price_id.clone(),
            portal_configuration: cfg.stripe_portal_configuration.clone(),
            webhook_secret: cfg.stripe_webhook_secret.clone(),
        })
    }
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        form: &[(&str, String)],
        idempotency: Option<&str>,
    ) -> Result<Value> {
        let mut request = self
            .http
            .request(method.clone(), format!("{}/v1{path}", self.base))
            .bearer_auth(&self.secret)
            .header("Stripe-Version", VERSION);
        request = if method == reqwest::Method::GET {
            request.query(form)
        } else {
            request.form(form)
        };
        if let Some(key) = idempotency {
            request = request.header("Idempotency-Key", key);
        }
        let mut response = request.send().await?;
        if !response.status().is_success() {
            bail!("Stripe request failed ({})", response.status().as_u16());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                bail!("Stripe response too large");
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).context("invalid Stripe response")
    }
    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.request(reqwest::Method::GET, path, query, None).await
    }
    pub async fn post(
        &self,
        path: &str,
        form: &[(&str, String)],
        idempotency: &str,
    ) -> Result<Value> {
        self.request(reqwest::Method::POST, path, form, Some(idempotency))
            .await
    }
    pub async fn validate_configuration(&self) -> Result<()> {
        if !valid_id(&self.price, "price_") || !valid_id(&self.portal_configuration, "bpc_") {
            bail!("invalid Stripe configuration IDs");
        }
        let price = self.get(&format!("/prices/{}", self.price), &[]).await?;
        if price["active"] != true
            || price["currency"] != "usd"
            || price["unit_amount"] != 1000
            || price["recurring"]["interval"] != "month"
            || price["recurring"]["interval_count"] != 1
            || price["recurring"]["usage_type"] != "licensed"
            || price["billing_scheme"] != "per_unit"
        {
            bail!("Standard price must be an active USD $10/month licensed price");
        }
        let portal = self
            .get(
                &format!(
                    "/billing_portal/configurations/{}",
                    self.portal_configuration
                ),
                &[],
            )
            .await?;
        if portal["active"] != true
            || portal["features"]["subscription_cancel"]["enabled"] != true
            || portal["features"]["subscription_cancel"]["mode"] != "at_period_end"
            || portal["features"]["subscription_update"]["enabled"] != false
        {
            bail!("Portal must allow period-end cancellation and disable plan/quantity changes");
        }
        Ok(())
    }
    pub fn verify_webhook(&self, header: &str, body: &[u8], now: i64) -> Result<Value> {
        verify_signature(&self.webhook_secret, header, body, now)?;
        let event: Value = serde_json::from_slice(body)?;
        let live = self.secret.starts_with("sk_live_") || self.secret.starts_with("rk_live_");
        if event["livemode"].as_bool() != Some(live) {
            bail!("wrong Stripe mode");
        }
        Ok(event)
    }
}
pub fn valid_id(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value.len() <= 255
        && value.len() > prefix.len()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub fn verify_signature(secret: &str, header: &str, body: &[u8], now: i64) -> Result<()> {
    if header.len() > 4096 {
        bail!("invalid signature");
    }
    let timestamps: Vec<_> = header
        .split(',')
        .filter_map(|p| p.strip_prefix("t="))
        .collect();
    if timestamps.len() != 1 {
        bail!("invalid timestamp");
    }
    let timestamp: i64 = timestamps[0].parse()?;
    if now.abs_diff(timestamp) > 300 {
        bail!("expired signature");
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    mac.update(timestamps[0].as_bytes());
    mac.update(b".");
    mac.update(body);
    for signature in header.split(',').filter_map(|p| p.strip_prefix("v1=")) {
        if let Ok(bytes) = hex::decode(signature) {
            if mac.clone().verify_slice(&bytes).is_ok() {
                return Ok(());
            }
        }
    }
    bail!("invalid signature")
}
