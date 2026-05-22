//! Environment configuration.
//!
//! All env vars are read once at startup. Missing required vars fail the
//! process with a clear message — no silent defaults.

use anyhow::{Context, Result};

use crate::rate;

#[derive(Clone, Debug)]
pub struct Config {
    /// Postgres connection string. On Fly.io, supplied via `fly secrets set`
    /// after attaching/creating a Postgres cluster (or via an external
    /// provider like Supabase / Neon).
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
    pub stripe_secret_key: String,
    /// Max cents a single grant can apply. Default $20.
    pub max_grant_cents: i64,
    /// Max cents one operator can grant in a rolling 24-hour window. Default $100.
    pub max_daily_per_operator_cents: i64,
    /// Discount for LITKEY payments, in basis points. Default 0. Example:
    /// 2000 = "20% off vs credit card".
    pub litkey_discount_basis_points: i64,
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
            max_grant_cents: optional_i64("MAX_GRANT_CENTS", 2_000)?,
            max_daily_per_operator_cents: optional_i64("MAX_DAILY_PER_OPERATOR_CENTS", 10_000)?,
            litkey_discount_basis_points: parse_discount_basis_points()?,
        })
    }
}

fn parse_discount_basis_points() -> Result<i64> {
    let bps = optional_i64("LITKEY_DISCOUNT_BASIS_POINTS", 0)?;
    rate::validate_discount_basis_points(bps)?;
    Ok(bps)
}

fn optional_i64(name: &str, default: i64) -> Result<i64> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<i64>()
            .with_context(|| format!("env var {name} must be an integer; got {v:?}")),
        _ => Ok(default),
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
