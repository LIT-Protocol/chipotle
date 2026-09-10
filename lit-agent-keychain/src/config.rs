//! Environment configuration.

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub magic_link_signing_key: Vec<u8>,
    pub resend_api_key: String,
    pub mail_from: String,
    pub public_base_url: String,
    /// AEAD key protecting Chipotle usage API keys (agent keys + per-tenant
    /// service keys) at rest. These are *Chipotle credentials*, not secret
    /// values — secret values are sealed inside the TEE and never stored here
    /// in plaintext.
    pub usage_key_encryption_key: Vec<u8>,
    pub chipotle_api_base_url: String,
    /// Master API key of the Chipotle account this deployment operates. Used
    /// only for management calls (mint vault PKPs, groups, usage keys). Every
    /// tenant lives on this one account.
    pub chipotle_master_api_key: String,
    /// secp256k1 private key (hex) that signs grants. Its address is baked into
    /// the reader action source, so rotating it changes the reader CID.
    pub grant_signing_key: String,
    pub grant_ttl_secs: i64,
    pub max_secret_bytes: usize,
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
            chipotle_master_api_key: required("CHIPOTLE_MASTER_API_KEY")?,
            grant_signing_key: required("GRANT_SIGNING_KEY")?,
            grant_ttl_secs: optional_parse_min("GRANT_TTL_SECS", 120, 10)?,
            max_secret_bytes: optional_parse_min("MAX_SECRET_BYTES", 16 * 1024, 1)?,
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

fn optional_parse_min<T>(name: &str, default: T, min: T) -> Result<T>
where
    T: std::str::FromStr + PartialOrd + Copy + std::fmt::Display,
    T::Err: std::fmt::Display,
{
    let value = match optional(name) {
        Some(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow::anyhow!("env var {name} has invalid value: {e}"))?,
        None => default,
    };
    if value < min {
        anyhow::bail!("env var {name} must be >= {min}");
    }
    Ok(value)
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
