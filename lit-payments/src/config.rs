//! Environment configuration.
//!
//! All env vars are read once at startup. Missing required vars fail the
//! process with a clear message — no silent defaults.

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    /// Postgres connection string. Railway provides this automatically.
    pub database_url: String,
    /// HMAC-SHA256 key for signing magic-link tokens. At least 32 random bytes,
    /// base64-encoded. Generate with `openssl rand -base64 32`.
    pub magic_link_signing_key: Vec<u8>,
    /// Resend API key (https://resend.com).
    pub resend_api_key: String,
    /// From: address on magic-link emails (e.g., "noreply@mail.litprotocol.com").
    pub mail_from: String,
    /// Public base URL, used to build magic-link verification URLs.
    /// e.g., "https://payments.litprotocol.com".
    pub public_base_url: String,
    /// Stripe restricted secret key for the ops-facing service.
    /// Not used yet by the foundation/auth code, but parsed eagerly so a
    /// misconfigured deploy fails on boot rather than on first request.
    pub stripe_secret_key: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            magic_link_signing_key: parse_signing_key()?,
            resend_api_key: required("RESEND_API_KEY")?,
            mail_from: required("MAIL_FROM")?,
            public_base_url: required("PUBLIC_BASE_URL")?
                .trim_end_matches('/')
                .to_string(),
            stripe_secret_key: required("STRIPE_SECRET_KEY")?,
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

fn parse_signing_key() -> Result<Vec<u8>> {
    use base64::Engine;
    let raw = required("MAGIC_LINK_SIGNING_KEY")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .context("MAGIC_LINK_SIGNING_KEY must be valid base64")?;
    if bytes.len() < 32 {
        anyhow::bail!(
            "MAGIC_LINK_SIGNING_KEY decodes to {} bytes; need at least 32. \
             Generate one with `openssl rand -base64 32`.",
            bytes.len()
        );
    }
    Ok(bytes)
}
