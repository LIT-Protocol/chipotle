use anyhow::{bail, Context, Result};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub public_base_url: String,
    pub lit_api_url: String,
    pub lit_execution_key: String,
    pub network: String,
    pub google_client_id: Option<String>,
    pub daily_execution_limit: i64,
    pub hourly_ip_execution_limit: i64,
    pub daily_vault_execution_limit: i64,
    pub max_secrets: i64,
    pub secure_cookies: bool,
    pub web_dir: String,
}
impl Config {
    pub fn from_env() -> Result<Self> {
        let public_base_url = required("PUBLIC_BASE_URL")?;
        validate_origin(&public_base_url)?;
        let lit_api_url = optional("LIT_API_URL")
            .unwrap_or_else(|| "https://api.chipotle.litprotocol.com".into());
        validate_origin(&lit_api_url)?;
        let secure_cookies = public_base_url.starts_with("https://");
        let max_secrets = positive("MAX_SECRETS_PER_VAULT", 500)?;
        if max_secrets > 500 {
            bail!("MAX_SECRETS_PER_VAULT must be at most 500");
        }
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            public_base_url,
            lit_api_url,
            lit_execution_key: required("LIT_EXECUTION_KEY")?,
            network: optional("LIT_NETWORK").unwrap_or_else(|| "chipotle-v1".into()),
            google_client_id: optional("GOOGLE_CLIENT_ID"),
            daily_execution_limit: positive("DAILY_EXECUTION_LIMIT", 10000)?,
            hourly_ip_execution_limit: positive("HOURLY_IP_EXECUTION_LIMIT", 200)?,
            daily_vault_execution_limit: positive("DAILY_VAULT_EXECUTION_LIMIT", 1000)?,
            max_secrets,
            secure_cookies,
            web_dir: optional("WEB_DIR").unwrap_or_else(|| "web/dist".into()),
        })
    }
}
fn required(key: &str) -> Result<String> {
    optional(key).with_context(|| format!("missing {key}"))
}
fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}
fn positive(key: &str, default: i64) -> Result<i64> {
    let value = optional(key)
        .map(|s| s.parse::<i64>())
        .transpose()?
        .unwrap_or(default);
    if !(1..=1_000_000).contains(&value) {
        bail!("{key} must be 1..1000000");
    }
    Ok(value)
}
pub fn validate_origin(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if value.len() > 256
        || url.origin().ascii_serialization() != value
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
    {
        bail!("expected HTTPS origin (HTTP is allowed only on loopback)");
    }
    Ok(())
}
