//! Dashboard password-login storage service (`/auth/v1/*`).
//!
//! The dashboard encrypts a user's master API key in the browser (Argon2id +
//! AES-256-GCM, see `lit-static/dapps/dashboard/password-protocol.js`) and
//! stores only the ciphertext here. This module owns verified email
//! identities, the password *verifier* (a hash of an already stretched,
//! domain-separated secret — never the password), the signup state machine,
//! sessions, email verification links and the encrypted mail outbox.
//!
//! It is a port of the former Cloudflare Worker in `lit-dashboard-auth/`; the
//! HTTP contract (`lit-dashboard-auth/openapi.json`) is unchanged so the
//! dashboard's `password-client.js` needs no changes. Design and operator
//! steps: `plans/dashboard-auth-on-lit-payments.md`.
//!
//! The feature is off unless `DASHBOARD_AUTH_SECRET` is configured; every
//! `/auth/v1/*` request then fails closed with 503.

pub mod crypto;
pub mod db;
pub mod outbox;
pub mod protocol;
pub mod routes;

use anyhow::{Context, Result, bail};
use rocket::http::{Cookie, SameSite};
use url::Url;

/// Session cookie. `__Host-` pins it to this origin, `Path=/`, `Secure`.
pub const COOKIE: &str = "__Host-chipotle_auth";
pub const SESSION_IDLE_SECONDS: i64 = 30 * 60;
pub const SESSION_MAX_SECONDS: i64 = 12 * 3600;
pub const SIGNUP_SESSION_MAX_SECONDS: i64 = 3600;
pub const SIGNUP_SESSION_IDLE_SECONDS: i64 = 30 * 60;
pub const VERIFICATION_TTL_SECONDS: i64 = 30 * 60;
pub const NOTIFICATION_TTL_SECONDS: i64 = 86400;
/// JSON request bodies above this size are rejected (413). Bodies are small
/// (an envelope is ~300 bytes); the cap bounds memory and parse work.
pub const MAX_BODY_BYTES: usize = 8192;
pub const MAX_OUTBOX_ATTEMPTS: i64 = 6;
pub const OUTBOX_RETRY_SECONDS: i64 = 300;

/// One allowed dashboard deployment: the exact `Origin` value requests must
/// carry and the full dashboard URL verification links point back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dashboard {
    pub origin: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookieSameSite {
    /// Default. Correct when the dashboard and lit-payments share a
    /// registrable domain (production: `*.litprotocol.com`).
    Lax,
    /// Required when they do not (staging: `*.up.railway.app` vs
    /// `*.pages.dev`). Emits `SameSite=None; Secure; Partitioned`. CSRF
    /// protection does not depend on this attribute: the exact `Origin`
    /// check, the custom header (forces a preflight) and the per-session
    /// CSRF token remain.
    None,
}

#[derive(Clone)]
pub struct DashboardAuthConfig {
    /// Service secret (≥32 chars). HMACs rate-limit keys and synthetic login
    /// parameters and derives the outbox encryption key. It cannot decrypt
    /// account envelopes. Distinct per environment.
    pub secret: String,
    pub dashboards: Vec<Dashboard>,
    /// `^[a-z0-9-]{1,40}$`. Part of every envelope's AAD; changing it orphans
    /// existing ciphertext.
    pub environment: String,
    pub cookie_same_site: CookieSameSite,
    /// Header carrying the client address. The *last* comma-separated value
    /// is used (the one appended by the trusted edge proxy).
    pub client_ip_header: String,
    pub outbox_interval_secs: u64,
}

impl std::fmt::Debug for DashboardAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DashboardAuthConfig")
            .field("dashboards", &self.dashboards)
            .field("environment", &self.environment)
            .field("cookie_same_site", &self.cookie_same_site)
            .field("client_ip_header", &self.client_ip_header)
            .field("outbox_interval_secs", &self.outbox_interval_secs)
            .finish_non_exhaustive()
    }
}

impl DashboardAuthConfig {
    /// `None` when `DASHBOARD_AUTH_SECRET` is unset (feature off). Once it is
    /// set the other required variables fail loudly at boot.
    pub fn from_env() -> Result<Option<Self>> {
        let Some(secret) = env_trimmed("DASHBOARD_AUTH_SECRET") else {
            return Ok(None);
        };
        let urls = env_trimmed("DASHBOARD_AUTH_URLS")
            .context("DASHBOARD_AUTH_URLS is required when DASHBOARD_AUTH_SECRET is set")?;
        let environment = env_trimmed("DASHBOARD_AUTH_ENVIRONMENT")
            .context("DASHBOARD_AUTH_ENVIRONMENT is required when DASHBOARD_AUTH_SECRET is set")?;
        let same_site = env_trimmed("DASHBOARD_AUTH_COOKIE_SAMESITE");
        let ip_header = env_trimmed("DASHBOARD_AUTH_CLIENT_IP_HEADER");
        let interval = match env_trimmed("DASHBOARD_AUTH_OUTBOX_INTERVAL_SECS") {
            Some(v) => Some(v.parse::<u64>().with_context(|| {
                format!("DASHBOARD_AUTH_OUTBOX_INTERVAL_SECS must be an integer; got {v:?}")
            })?),
            None => None,
        };
        Self::parse(
            secret,
            &urls,
            environment,
            same_site.as_deref(),
            ip_header,
            interval,
        )
        .map(Some)
    }

    pub fn parse(
        secret: String,
        urls: &str,
        environment: String,
        same_site: Option<&str>,
        client_ip_header: Option<String>,
        outbox_interval_secs: Option<u64>,
    ) -> Result<Self> {
        if secret.len() < 32 {
            bail!("DASHBOARD_AUTH_SECRET must be at least 32 characters");
        }
        if !protocol::valid_environment(&environment) {
            bail!("DASHBOARD_AUTH_ENVIRONMENT must match ^[a-z0-9-]{{1,40}}$; got {environment:?}");
        }
        let mut dashboards = Vec::new();
        for raw in urls.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let dashboard = parse_dashboard_url(raw)?;
            if dashboards
                .iter()
                .any(|d: &Dashboard| d.origin == dashboard.origin)
            {
                bail!(
                    "DASHBOARD_AUTH_URLS lists origin {} twice",
                    dashboard.origin
                );
            }
            dashboards.push(dashboard);
        }
        if dashboards.is_empty() {
            bail!("DASHBOARD_AUTH_URLS must list at least one dashboard URL");
        }
        let cookie_same_site = match same_site.map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("lax") => CookieSameSite::Lax,
            Some("none") => CookieSameSite::None,
            Some(other) => {
                bail!("DASHBOARD_AUTH_COOKIE_SAMESITE must be 'lax' or 'none'; got {other:?}")
            }
        };
        let client_ip_header = client_ip_header.unwrap_or_else(|| "X-Forwarded-For".to_string());
        let outbox_interval_secs = outbox_interval_secs.unwrap_or(300).max(5);
        Ok(Self {
            secret,
            dashboards,
            environment,
            cookie_same_site,
            client_ip_header,
            outbox_interval_secs,
        })
    }

    pub fn dashboard_for_origin(&self, origin: &str) -> Option<&Dashboard> {
        self.dashboards.iter().find(|d| d.origin == origin)
    }

    pub fn origins(&self) -> impl Iterator<Item = &str> {
        self.dashboards.iter().map(|d| d.origin.as_str())
    }

    /// Build the session cookie. `max_age` of zero clears it; the same
    /// attributes are repeated so browsers match the stored cookie
    /// (partitioned cookies in particular are only overwritten by a
    /// partitioned `Set-Cookie`).
    pub fn session_cookie(&self, value: String, max_age_secs: i64) -> Cookie<'static> {
        let builder = Cookie::build((COOKIE, value))
            .path("/")
            .secure(true)
            .http_only(true)
            .max_age(time::Duration::seconds(max_age_secs));
        match self.cookie_same_site {
            CookieSameSite::Lax => builder.same_site(SameSite::Lax),
            CookieSameSite::None => builder.same_site(SameSite::None).partitioned(true),
        }
        .build()
    }
}

/// A dashboard URL must be an absolute `https://` URL (plain `http://` is only
/// accepted for loopback test servers) ending in `/dapps/dashboard/`, with no
/// credentials, query or fragment: the verification link is built by
/// appending a fragment to it.
fn parse_dashboard_url(raw: &str) -> Result<Dashboard> {
    let url = Url::parse(raw).with_context(|| format!("invalid dashboard URL {raw:?}"))?;
    let host = url.host_str().unwrap_or_default();
    let loopback = matches!(host, "localhost" | "127.0.0.1");
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => bail!("dashboard URL {raw:?} must use https"),
    }
    if host.is_empty() || !url.username().is_empty() || url.password().is_some() {
        bail!("dashboard URL {raw:?} must have a host and no credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("dashboard URL {raw:?} must not have a query or fragment");
    }
    if url.path() != "/dapps/dashboard/" {
        bail!("dashboard URL {raw:?} must end in /dapps/dashboard/");
    }
    let origin = url.origin().ascii_serialization();
    Ok(Dashboard {
        origin,
        url: url.to_string(),
    })
}

fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// A request rejected with a specific status and user-facing message. The
/// strings are part of the dashboard contract (the UI shows them verbatim).
#[derive(Debug)]
pub struct Failure {
    pub status: u16,
    pub message: String,
}

pub fn fail<T>(status: u16, message: &str) -> Result<T, Failure> {
    Err(Failure {
        status,
        message: message.to_string(),
    })
}

pub const UNAVAILABLE: &str = "Account service unavailable. Please try again.";

impl From<anyhow::Error> for Failure {
    fn from(e: anyhow::Error) -> Self {
        log_internal_error(&e);
        Failure {
            status: 503,
            message: UNAVAILABLE.to_string(),
        }
    }
}

impl From<sqlx::Error> for Failure {
    fn from(e: sqlx::Error) -> Self {
        Failure::from(anyhow::Error::from(e))
    }
}

/// Request bodies, tokens, ciphertexts and upstream error text are never
/// logged. Database errors are reduced to their SQLSTATE code.
fn log_internal_error(e: &anyhow::Error) {
    let code = e
        .downcast_ref::<sqlx::Error>()
        .and_then(|e| e.as_database_error())
        .and_then(|d| d.code().map(|c| c.to_string()));
    match code {
        Some(code) => tracing::warn!(sqlstate = %code, "dashboard_auth_request_failed"),
        None => tracing::warn!("dashboard_auth_request_failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> String {
        "test-only-secret-not-for-production-12345".to_string()
    }

    #[test]
    fn parses_dashboard_lists_and_origins() {
        let cfg = DashboardAuthConfig::parse(
            secret(),
            "https://lit-static-next.pages.dev/dapps/dashboard/, https://dashboard.dev.litprotocol.com/dapps/dashboard/",
            "staging".into(),
            Some("none"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(cfg.dashboards.len(), 2);
        assert_eq!(
            cfg.dashboards[0].origin,
            "https://lit-static-next.pages.dev"
        );
        assert_eq!(
            cfg.dashboards[1].url,
            "https://dashboard.dev.litprotocol.com/dapps/dashboard/"
        );
        assert_eq!(cfg.cookie_same_site, CookieSameSite::None);
        assert_eq!(cfg.client_ip_header, "X-Forwarded-For");
        assert_eq!(cfg.outbox_interval_secs, 300);
        assert!(
            cfg.dashboard_for_origin("https://lit-static-next.pages.dev")
                .is_some()
        );
        assert!(cfg.dashboard_for_origin("https://evil.test").is_none());
    }

    #[test]
    fn rejects_bad_config() {
        let ok = "https://dashboard.example/dapps/dashboard/";
        assert!(
            DashboardAuthConfig::parse("short".into(), ok, "staging".into(), None, None, None)
                .is_err()
        );
        assert!(
            DashboardAuthConfig::parse(secret(), ok, "Staging!".into(), None, None, None).is_err()
        );
        assert!(
            DashboardAuthConfig::parse(secret(), "", "staging".into(), None, None, None).is_err()
        );
        assert!(
            DashboardAuthConfig::parse(secret(), ok, "staging".into(), Some("strict"), None, None)
                .is_err()
        );
        for bad in [
            "http://dashboard.example/dapps/dashboard/",
            "https://user:pw@dashboard.example/dapps/dashboard/",
            "https://dashboard.example/dapps/dashboard/?x=1",
            "https://dashboard.example/dapps/dashboard/#frag",
            "https://dashboard.example/",
            "https://dashboard.example/dapps/dashboard",
        ] {
            assert!(
                DashboardAuthConfig::parse(secret(), bad, "staging".into(), None, None, None)
                    .is_err(),
                "{bad} should be rejected"
            );
        }
        assert!(
            DashboardAuthConfig::parse(
                secret(),
                "http://localhost:8080/dapps/dashboard/",
                "local".into(),
                None,
                None,
                None
            )
            .is_ok()
        );
        assert!(
            DashboardAuthConfig::parse(
                secret(),
                &format!("{ok},{ok}"),
                "staging".into(),
                None,
                None,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn session_cookie_attributes_follow_same_site_setting() {
        let lax = DashboardAuthConfig::parse(
            secret(),
            "https://dashboard.example/dapps/dashboard/",
            "staging".into(),
            None,
            None,
            None,
        )
        .unwrap();
        let cookie = lax.session_cookie("abc".into(), 60).to_string();
        assert!(cookie.starts_with("__Host-chipotle_auth=abc;"));
        assert!(
            cookie.contains("HttpOnly") && cookie.contains("Secure") && cookie.contains("Path=/")
        );
        assert!(cookie.contains("SameSite=Lax") && !cookie.contains("Partitioned"));
        let none = DashboardAuthConfig::parse(
            secret(),
            "https://dashboard.example/dapps/dashboard/",
            "staging".into(),
            Some("none"),
            None,
            None,
        )
        .unwrap();
        let cookie = none.session_cookie(String::new(), 0).to_string();
        assert!(
            cookie.contains("SameSite=None")
                && cookie.contains("Partitioned")
                && cookie.contains("Max-Age=0")
        );
    }
}
