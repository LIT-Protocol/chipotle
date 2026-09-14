use anyhow::{bail, Context, Result};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub public_base_url: String,
    pub lit_api_url: String,
    pub lit_execution_key: String,
    pub chipotle_master_key: String,
    pub usage_key_encryption_key: [u8; 32],
    pub stripe_secret_key: String,
    pub stripe_webhook_secret: String,
    pub stripe_price_id: String,
    pub stripe_portal_configuration: String,
    pub stripe_api_url: String,
    pub contact_email: String,
    pub network: String,
    pub google_client_id: Option<String>,
    pub daily_execution_limit: i64,
    pub hourly_ip_execution_limit: i64,
    pub daily_vault_execution_limit: i64,
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
        let encryption_key = hex::decode(required("USAGE_KEY_ENCRYPTION_KEY")?)?;
        let usage_key_encryption_key: [u8; 32] = encryption_key
            .try_into()
            .map_err(|_| anyhow::anyhow!("USAGE_KEY_ENCRYPTION_KEY must be 32 bytes of hex"))?;
        let stripe_api_url =
            optional("KEYCHAIN_STRIPE_API_URL").unwrap_or_else(|| "https://api.stripe.com".into());
        if stripe_api_url != "https://api.stripe.com" {
            validate_origin(&stripe_api_url)?;
            if secure_cookies || !stripe_api_url.starts_with("http://") {
                bail!("Stripe API override is only allowed for loopback development");
            }
        }
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            public_base_url,
            lit_api_url,
            lit_execution_key: required("LIT_EXECUTION_KEY")?,
            chipotle_master_key: required("CHIPOTLE_MASTER_API_KEY")?,
            usage_key_encryption_key,
            stripe_secret_key: required("STRIPE_SECRET_KEY")?,
            stripe_webhook_secret: required("STRIPE_WEBHOOK_SECRET")?,
            stripe_price_id: required("STRIPE_PRICE_ID")?,
            stripe_portal_configuration: required("STRIPE_PORTAL_CONFIGURATION_ID")?,
            stripe_api_url,
            contact_email: optional("CONTACT_EMAIL")
                .unwrap_or_else(|| "support@litprotocol.com".into()),
            network: optional("LIT_NETWORK").unwrap_or_else(|| "chipotle-v1".into()),
            google_client_id: optional("GOOGLE_CLIENT_ID"),
            daily_execution_limit: positive("DAILY_EXECUTION_LIMIT", 10000)?,
            hourly_ip_execution_limit: positive("HOURLY_IP_EXECUTION_LIMIT", 200)?,
            daily_vault_execution_limit: positive("DAILY_VAULT_EXECUTION_LIMIT", 1000)?,
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
