//! Environment configuration.

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub magic_link_signing_key: Vec<u8>,
    pub resend_api_key: String,
    pub mail_from: String,
    pub public_base_url: String,
    /// Base64-encoded AEAD key used to encrypt scoped Chipotle usage API keys.
    pub usage_key_encryption_key: Vec<u8>,
    pub chipotle_api_base_url: String,
    pub webhook_max_body_bytes: usize,
    pub webhook_ip_max_requests_per_minute: u32,
    pub webhook_user_max_requests_per_minute: u32,
    pub webhook_trigger_max_requests_per_minute: u32,
    pub webhook_default_max_queued_runs: u32,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            magic_link_signing_key: parse_b64_key("MAGIC_LINK_SIGNING_KEY")?,
            resend_api_key: required("RESEND_API_KEY")?,
            mail_from: required("MAIL_FROM")?,
            public_base_url: required("PUBLIC_BASE_URL")?
                .trim_end_matches('/')
                .to_string(),
            usage_key_encryption_key: parse_b64_key("USAGE_KEY_ENCRYPTION_KEY")?,
            chipotle_api_base_url: optional("CHIPOTLE_API_BASE_URL")
                .unwrap_or_else(|| "https://api.chipotle.litprotocol.com".to_string())
                .trim_end_matches('/')
                .to_string(),
            webhook_max_body_bytes: optional_parse("WEBHOOK_MAX_BODY_BYTES", 256 * 1024)?,
            webhook_ip_max_requests_per_minute: optional_parse(
                "WEBHOOK_IP_MAX_REQUESTS_PER_MINUTE",
                60,
            )?,
            webhook_user_max_requests_per_minute: optional_parse(
                "WEBHOOK_USER_MAX_REQUESTS_PER_MINUTE",
                120,
            )?,
            webhook_trigger_max_requests_per_minute: optional_parse(
                "WEBHOOK_TRIGGER_MAX_REQUESTS_PER_MINUTE",
                60,
            )?,
            webhook_default_max_queued_runs: optional_parse(
                "WEBHOOK_DEFAULT_MAX_QUEUED_RUNS",
                100,
            )?,
        })
    }
}

fn required(name: &str) -> Result<String> {
    let v = std::env::var(name).with_context(|| format!("missing env var: {name}"))?;
    if v.trim().is_empty() {
        anyhow::bail!("env var {name} is empty");
    }
    Ok(v)
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn optional_parse<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match optional(name) {
        Some(value) => value
            .parse::<T>()
            .map_err(|e| anyhow::anyhow!("env var {name} has invalid value: {e}")),
        None => Ok(default),
    }
}

fn parse_b64_key(name: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    let raw = required(name)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .with_context(|| format!("{name} must be valid base64"))?;
    if bytes.len() < 32 {
        anyhow::bail!(
            "{name} decodes to {} bytes; need at least 32. Generate one with `openssl rand -base64 32`.",
            bytes.len()
        );
    }
    Ok(bytes)
}
