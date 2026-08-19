//! Environment-driven configuration. Fails fast on missing required vars.

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    /// PEM bundle for `sslmode=verify-full` to Railway Postgres. When set,
    /// TLS is mandatory and the server cert must chain to this CA.
    pub db_ca_cert_pem: Option<Vec<u8>>,
    pub public_base_url: String,
    /// Resend credentials; when absent, magic-link login is disabled
    /// (anonymous chat still works).
    pub resend_api_key: Option<String>,
    pub mail_from: Option<String>,
    pub openrouter_base_url: String,
    /// Bootstrap only (section 4.4): imported into the encrypted
    /// provider_keys store on first boot, then ignored.
    pub bootstrap_provisioning_key: Option<String>,
    pub bootstrap_runtime_key: Option<String>,
    /// Comma-separated emails granted admin at boot (MAC'd roster rows).
    pub bootstrap_admins: Vec<String>,
    /// Global daily spend circuit breaker (micro-USD). Default $50/day.
    pub daily_spend_cap_micro_usd: i64,
    /// Anonymous per-user daily token budget. Default 50k tokens.
    pub anon_daily_token_budget: i64,
    /// Cost-plus markup in basis points (500 = 5%, the section 8 placeholder).
    pub markup_bps: i64,
    /// Hard cap on max_tokens per generation.
    pub max_tokens_cap: i64,
    /// WebAuthn relying-party id + origin for the admin console.
    pub admin_rp_id: Option<String>,
    pub admin_origin: Option<String>,
    /// Dev-only: log magic-link codes instead of emailing (compiled out in
    /// production builds).
    pub dev_echo_codes: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let db_ca_cert_pem = match optional("LIT_CHAT_DB_CA_CERT_FILE") {
            Some(path) => Some(
                std::fs::read(&path)
                    .with_context(|| format!("reading LIT_CHAT_DB_CA_CERT_FILE at {path}"))?,
            ),
            None => optional("LIT_CHAT_DB_CA_CERT").map(|pem| pem.into_bytes()),
        };
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            db_ca_cert_pem,
            public_base_url: required("PUBLIC_BASE_URL")?
                .trim_end_matches('/')
                .to_string(),
            resend_api_key: optional("RESEND_API_KEY"),
            mail_from: optional("MAIL_FROM"),
            openrouter_base_url: optional("OPENROUTER_BASE_URL")
                .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string())
                .trim_end_matches('/')
                .to_string(),
            bootstrap_provisioning_key: optional("OPENROUTER_PROVISIONING_KEY"),
            bootstrap_runtime_key: optional("OPENROUTER_API_KEY"),
            bootstrap_admins: optional("LIT_CHAT_BOOTSTRAP_ADMINS")
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            daily_spend_cap_micro_usd: optional_parse("LIT_CHAT_DAILY_SPEND_CAP_MICRO_USD")
                .unwrap_or(50_000_000),
            anon_daily_token_budget: optional_parse("LIT_CHAT_ANON_DAILY_TOKENS").unwrap_or(50_000),
            markup_bps: optional_parse("LIT_CHAT_MARKUP_BPS").unwrap_or(500),
            max_tokens_cap: optional_parse("LIT_CHAT_MAX_TOKENS_CAP").unwrap_or(4096),
            admin_rp_id: optional("LIT_CHAT_ADMIN_RP_ID"),
            admin_origin: optional("LIT_CHAT_ADMIN_ORIGIN"),
            dev_echo_codes: dev_echo_codes(),
        })
    }
}

#[cfg(feature = "production")]
fn dev_echo_codes() -> bool {
    false
}

#[cfg(not(feature = "production"))]
fn dev_echo_codes() -> bool {
    optional("LIT_CHAT_DEV_ECHO_CODES")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
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

fn optional_parse<T: std::str::FromStr>(name: &str) -> Option<T> {
    optional(name).and_then(|v| v.trim().parse().ok())
}
