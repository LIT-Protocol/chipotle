//! Environment configuration.
//!
//! All env vars are read once at startup. Missing required vars fail the
//! process with a clear message — no silent defaults.

use std::str::FromStr;

use alloy_primitives::Address;
use anyhow::{Context, Result};
use lit_billing_core::on_chain::OnChainBillingResolver;

use crate::{chain, rate};

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
    /// Stripe publishable key — returned by `POST /billing/setup_intent` so
    /// the dashboard can initialise Stripe.js without baking the key into
    /// every front-end deploy. Pulled from the same Stripe account as
    /// `stripe_secret_key`; no harm in shipping to the client.
    pub stripe_publishable_key: String,
    /// Max cents a single grant can apply. Default $20.
    pub max_grant_cents: i64,
    /// Max cents one operator can grant in a rolling 24-hour window. Default $100.
    pub max_daily_per_operator_cents: i64,
    /// Discount for LITKEY payments, in basis points. Default 0. Example:
    /// 2000 = "20% off vs credit card".
    pub litkey_discount_basis_points: i64,
    /// Optional Base LITKEY chain verification configuration. If unset, the
    /// admin portal and rate poller run but LITKEY browser payments stay
    /// disabled.
    pub litkey_chain: Option<chain::ChainConfig>,
    /// Base URL of `lit-api-server`, used for the auto-top-up cache
    /// invalidation callback after a successful credit. e.g.,
    /// `https://api.litprotocol.com`. This is the ONLY remaining
    /// lit-api-server hop — wallet-sig verification + on-chain key
    /// resolution moved into the shared `lit-billing-core` crate after
    /// glitch's PR #448 architectural review.
    pub lit_api_server_base_url: String,
    /// JSON-RPC endpoint for the chain that hosts AccountConfig (the
    /// contract that maps API-key hashes → admin wallets, used by the
    /// `BillingAuth` guard's API-key path). Same chain `lit-api-server`
    /// configures via NodeConfig.toml's `[chain] name = …`. Required
    /// because lit-payments now runs the on-chain lookup in-process via
    /// `OnChainBillingResolver` (was an internal HTTP hop pre-#448
    /// follow-up).
    pub lit_accounts_rpc_url: String,
    /// Chain ID of the AccountConfig chain. Used both by the on-chain
    /// resolver above and by the EIP-712 verifier (the wallet-sig payload
    /// commits to a chain ID; signatures minted against a different chain
    /// will fail to recover). Read from `LIT_ACCOUNTS_CHAIN_ID`.
    pub lit_accounts_chain_id: u64,
    /// 0x-hex deployed address of the AccountConfig contract on the
    /// chain above. Mirrors lit-api-server's `NodeConfig.toml`
    /// `[chain] contract_address = …`. Read from
    /// `LIT_ACCOUNTS_CONTRACT_ADDRESS`.
    pub lit_accounts_contract_address: Address,
    /// High-entropy shared secret for the internal `lit-payments` ⇄
    /// `lit-api-server` hop. Compared in constant time against the
    /// `X-Internal-Secret` header on internal endpoints. Same value must be
    /// configured on both services. Generate with `openssl rand -base64 32`.
    pub lit_internal_shared_secret: String,
    /// Stripe webhook signing secret. Used to verify the `Stripe-Signature`
    /// header on `POST /stripe/webhook` (the single `customer.updated`
    /// event that triggers the auto-top-up flow). Copy from the Stripe
    /// Dashboard after registering the webhook endpoint, or from
    /// `stripe listen` output in local dev.
    pub stripe_webhook_secret: String,
    /// Interval (seconds) between reconciler runs (Phase 6). The reconciler
    /// walks recent auto-top-up PIs and credits any that the sync handler
    /// missed (HTTP timeout on `paymentIntents.create` or the
    /// `balance_transactions` write). Default 900 (15 minutes); set lower
    /// (e.g. 60) for local testing.
    pub reconciler_interval_secs: i64,
    /// Interval (seconds) between enterprise committed-use billing sweeps. Each
    /// sweep checks every active `enterprise_accounts` row and, when its anchor
    /// day has arrived, drafts that cycle's invoice. Hourly is plenty (the work
    /// is per-period idempotent); set lower for local testing. Default 3600.
    pub enterprise_billing_interval_secs: i64,
    /// Base URL for Stripe dashboard links in the enterprise review email.
    /// Live: `https://dashboard.stripe.com`; test mode:
    /// `https://dashboard.stripe.com/test`. Default is the live base.
    pub stripe_dashboard_base: String,
    /// Exact-match CORS allowlist. Browser requests from any other origin
    /// will be rejected at the preflight gate. Default in production is
    /// the `PUBLIC_BASE_URL` (the dashboard is served from the same
    /// origin in production builds); local dev typically sets this to
    /// the Vite dev-server origin via `CORS_ALLOWED_ORIGINS`.
    ///
    /// Codex P1 (Phase 8): the prior config used
    /// `AllowedOrigins::all()` which lets any site initiate
    /// credentialed requests against lit-payments. Combined with
    /// `allow_credentials: true` that's CSRF-on-tap. Now exact-match
    /// only; misconfigured deployments fail loudly.
    pub cors_allowed_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let public_base_url = required("PUBLIC_BASE_URL")?
            .trim_end_matches('/')
            .to_string();
        let cors_allowed_origins = parse_cors_allowed_origins(&public_base_url);
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            magic_link_signing_key: parse_signing_key()?,
            resend_api_key: required("RESEND_API_KEY")?,
            mail_from: required("MAIL_FROM")?,
            public_base_url,
            stripe_secret_key: required("STRIPE_SECRET_KEY")?,
            stripe_publishable_key: required("STRIPE_PUBLISHABLE_KEY")?,
            max_grant_cents: optional_i64("MAX_GRANT_CENTS", 2_000)?,
            max_daily_per_operator_cents: optional_i64("MAX_DAILY_PER_OPERATOR_CENTS", 10_000)?,
            litkey_discount_basis_points: parse_discount_basis_points()?,
            litkey_chain: parse_litkey_chain_config()?,
            lit_api_server_base_url: required("LIT_API_SERVER_BASE_URL")?
                .trim_end_matches('/')
                .to_string(),
            lit_internal_shared_secret: required("LIT_INTERNAL_SHARED_SECRET")?,
            lit_accounts_rpc_url: required("LIT_ACCOUNTS_RPC_URL")?,
            lit_accounts_chain_id: parse_chain_id()?,
            lit_accounts_contract_address: parse_accounts_contract_address()?,
            stripe_webhook_secret: required("STRIPE_WEBHOOK_SECRET")?,
            reconciler_interval_secs: optional_i64("RECONCILER_INTERVAL_SECS", 900)?,
            enterprise_billing_interval_secs: optional_i64(
                "ENTERPRISE_BILLING_INTERVAL_SECS",
                3600,
            )?,
            stripe_dashboard_base: optional_trimmed("STRIPE_DASHBOARD_BASE")
                .unwrap_or_else(|| "https://dashboard.stripe.com".to_string()),
            cors_allowed_origins,
        })
    }
}

/// Build the CORS allowlist. Always includes `public_base_url`
/// (same-origin = the production dashboard). The optional
/// `CORS_ALLOWED_ORIGINS` env var (comma-separated) adds additional
/// origins — the Vite dev server origin in local dev, or a separate
/// dashboard host in staging. The defaults intentionally do NOT
/// include `localhost:*` blindly; explicit allowlisting is the point.
fn parse_cors_allowed_origins(public_base_url: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![public_base_url.trim_end_matches('/').to_string()];
    if let Some(extra) = optional_trimmed("CORS_ALLOWED_ORIGINS") {
        for origin in extra.split(',') {
            let o = origin.trim().trim_end_matches('/').to_string();
            if !o.is_empty() && !out.contains(&o) {
                out.push(o);
            }
        }
    }
    out
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

fn optional_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Parse `LIT_ACCOUNTS_CHAIN_ID` as a `u64`. Required: signature
/// recovery against the wrong chain silently fails (the digest differs),
/// so an unset chain id would just produce 401s for every valid caller —
/// a hard failure at startup is friendlier.
fn parse_chain_id() -> Result<u64> {
    let raw = required("LIT_ACCOUNTS_CHAIN_ID")?;
    raw.trim()
        .parse::<u64>()
        .with_context(|| format!("LIT_ACCOUNTS_CHAIN_ID must be a positive integer; got {raw:?}"))
}

/// Parse `LIT_ACCOUNTS_CONTRACT_ADDRESS` as a 0x-prefixed Ethereum address.
/// Same content lit-api-server gets from `NodeConfig.toml`.
fn parse_accounts_contract_address() -> Result<Address> {
    let raw = required("LIT_ACCOUNTS_CONTRACT_ADDRESS")?;
    Address::from_str(raw.trim()).with_context(|| {
        format!("LIT_ACCOUNTS_CONTRACT_ADDRESS must be a 0x-hex Ethereum address; got {raw:?}")
    })
}

/// Build the on-chain resolver from the parsed config. Wraps the
/// `OnChainBillingResolver` constructor so callers (e.g. `main.rs`) don't
/// have to re-read env vars or clone strings around.
pub fn build_on_chain_resolver(cfg: &Config) -> Result<OnChainBillingResolver> {
    OnChainBillingResolver::new(
        cfg.lit_accounts_rpc_url.clone(),
        cfg.lit_accounts_contract_address,
    )
    .context("building OnChainBillingResolver from Config")
}

pub fn validate_chain_runtime_config(chain_id: i64) -> Result<()> {
    if chain_id != chain::BASE_CHAIN_ID {
        anyhow::bail!(
            "LITKEY_CHAIN_ID must be {} (Base mainnet)",
            chain::BASE_CHAIN_ID
        );
    }
    Ok(())
}

fn parse_litkey_chain_config() -> Result<Option<chain::ChainConfig>> {
    let https = optional_trimmed("ALCHEMY_HTTPS_URL");
    let gateway = optional_trimmed("LITKEY_GATEWAY_ADDRESS");

    if https.is_none() && gateway.is_none() {
        return Ok(None);
    }

    let https =
        https.context("ALCHEMY_HTTPS_URL is required when enabling LITKEY chain verification")?;
    let gateway = gateway
        .context("LITKEY_GATEWAY_ADDRESS is required when enabling LITKEY chain verification")?;
    let gateway_address = Address::from_str(&gateway)
        .with_context(|| format!("LITKEY_GATEWAY_ADDRESS must be a 0x address; got {gateway:?}"))?;

    let chain_id = optional_i64("LITKEY_CHAIN_ID", chain::BASE_CHAIN_ID)?;
    validate_chain_runtime_config(chain_id)?;

    Ok(Some(chain::ChainConfig {
        chain_id,
        alchemy_https_url: https,
        gateway_address,
    }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_litkey_chain_runtime_config_bounds() {
        assert!(validate_chain_runtime_config(chain::BASE_CHAIN_ID).is_ok());
        assert!(validate_chain_runtime_config(0).is_err());
    }
}
