//! Request/response + row shapes for per-key spending rules and rolling usage.
//!
//! See `plans/chipotle-lambda-parity.md`. The gateway reads these to enforce a
//! rolling spend cap, rate/concurrency limits, and an origin allowlist on
//! frontend-callable usage keys.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;

/// A row of `spending_rules` — the configured limits for one API key.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SpendingRules {
    pub api_key_hash: String,
    pub account_wallet_address: Option<String>,
    pub spend_cap_cents: Option<i64>,
    pub spend_window_seconds: Option<i64>,
    pub rate_limit_rps: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub max_concurrency: Option<i32>,
    pub ip_rate_limit_rps: Option<i32>,
    pub ip_rate_limit_burst: Option<i32>,
    pub allowed_origins: Option<Vec<String>>,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// A row of `spending_usage` — the rolling spend counter for one API key.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SpendingUsage {
    pub api_key_hash: String,
    #[serde(with = "time::serde::rfc3339")]
    pub window_started_at: OffsetDateTime,
    pub spent_cents: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Body of `PUT /api/spending-rules/<hash>` (operator) — the editable fields.
/// Omitted fields default to "no limit". Paired fields (cap+window, rps+burst)
/// must be supplied together; validated before write.
#[derive(Debug, Default, Deserialize)]
pub struct UpsertRulesRequest {
    #[serde(default)]
    pub account_wallet_address: Option<String>,
    #[serde(default)]
    pub spend_cap_cents: Option<i64>,
    #[serde(default)]
    pub spend_window_seconds: Option<i64>,
    #[serde(default)]
    pub rate_limit_rps: Option<i32>,
    #[serde(default)]
    pub rate_limit_burst: Option<i32>,
    #[serde(default)]
    pub max_concurrency: Option<i32>,
    /// Per-client-IP token bucket, enforced in addition to the per-key one.
    #[serde(default)]
    pub ip_rate_limit_rps: Option<i32>,
    #[serde(default)]
    pub ip_rate_limit_burst: Option<i32>,
    /// Browser `Origin` values allowed to use this key. `None`/empty = no
    /// origin check. Entries are `scheme://host[:port]`; a leading `*.` on the
    /// host matches any subdomain. Requests without an `Origin` header are
    /// rejected when this is set.
    #[serde(default)]
    pub allowed_origins: Option<Vec<String>>,
    /// Defaults to enabled when omitted.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// What the gateway fetches for its rules cache: the rules plus current usage.
#[derive(Debug, Serialize)]
pub struct RulesWithUsage {
    pub rules: SpendingRules,
    pub usage: Option<SpendingUsage>,
}

/// Body of the internal `POST /internal/spending-usage/<hash>/charge`. The
/// gateway supplies the window length (it has the rules cached) so the counter
/// can self-reset without a second query.
#[derive(Debug, Deserialize)]
pub struct ChargeRequest {
    pub cents: i64,
    pub window_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct RulesListResponse {
    pub rules: Vec<SpendingRules>,
}

#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl UpsertRulesRequest {
    /// Reject incomplete pairs / non-positive values before they hit the DB
    /// CHECK constraints, so callers get a clear 400 instead of a 500.
    pub fn validate(&self) -> Result<(), String> {
        if self.spend_cap_cents.is_some() != self.spend_window_seconds.is_some() {
            return Err("spend_cap_cents and spend_window_seconds must be set together".into());
        }
        if self.rate_limit_rps.is_some() != self.rate_limit_burst.is_some() {
            return Err("rate_limit_rps and rate_limit_burst must be set together".into());
        }
        if self.ip_rate_limit_rps.is_some() != self.ip_rate_limit_burst.is_some() {
            return Err("ip_rate_limit_rps and ip_rate_limit_burst must be set together".into());
        }
        for (name, v) in [
            ("spend_cap_cents", self.spend_cap_cents),
            ("spend_window_seconds", self.spend_window_seconds),
        ] {
            if let Some(v) = v
                && v <= 0
            {
                return Err(format!("{name} must be positive"));
            }
        }
        for (name, v) in [
            ("rate_limit_rps", self.rate_limit_rps),
            ("rate_limit_burst", self.rate_limit_burst),
            ("max_concurrency", self.max_concurrency),
            ("ip_rate_limit_rps", self.ip_rate_limit_rps),
            ("ip_rate_limit_burst", self.ip_rate_limit_burst),
        ] {
            if let Some(v) = v
                && v <= 0
            {
                return Err(format!("{name} must be positive"));
            }
        }
        if let Some(origins) = &self.allowed_origins {
            for o in origins {
                validate_origin_pattern(o)?;
            }
        }
        Ok(())
    }
}

/// An allowed-origin entry must be `scheme://host[:port]` with no path, query
/// or fragment — exactly the shape a browser sends in `Origin`. The host may
/// start with `*.` to match any subdomain. Rejects empty/whitespace entries.
pub fn validate_origin_pattern(raw: &str) -> Result<(), String> {
    let o = raw.trim();
    if o.is_empty() {
        return Err("allowed_origins must not contain empty entries".into());
    }
    let Some((scheme, rest)) = o.split_once("://") else {
        return Err(format!(
            "allowed_origins entry {o:?} must be scheme://host[:port]"
        ));
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err(format!(
            "allowed_origins entry {o:?} must use http or https"
        ));
    }
    if rest.is_empty() || rest.contains(['/', '?', '#', '@', ' ']) {
        return Err(format!(
            "allowed_origins entry {o:?} must not contain a path, query, credentials or fragment"
        ));
    }
    let host = rest.strip_prefix("*.").unwrap_or(rest);
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    if host.is_empty() || host.contains('*') {
        return Err(format!(
            "allowed_origins entry {o:?}: wildcard is only allowed as a leading `*.`"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> UpsertRulesRequest {
        UpsertRulesRequest {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn empty_rules_are_valid() {
        assert!(base().validate().is_ok());
    }

    #[test]
    fn spend_cap_requires_both_halves() {
        let mut r = base();
        r.spend_cap_cents = Some(1000);
        assert!(r.validate().is_err());
        r.spend_window_seconds = Some(86_400);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn rate_limit_requires_both_halves() {
        let mut r = base();
        r.rate_limit_rps = Some(10);
        assert!(r.validate().is_err());
        r.rate_limit_burst = Some(20);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn rejects_non_positive() {
        let mut r = base();
        r.max_concurrency = Some(0);
        assert!(r.validate().is_err());
    }

    #[test]
    fn rejects_empty_origin() {
        let mut r = base();
        r.allowed_origins = Some(vec!["https://app.example.com".into(), "  ".into()]);
        assert!(r.validate().is_err());
    }

    #[test]
    fn ip_rate_limit_requires_both_halves() {
        let mut r = base();
        r.ip_rate_limit_burst = Some(5);
        assert!(r.validate().is_err());
        r.ip_rate_limit_rps = Some(1);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn origin_patterns() {
        for ok in [
            "https://app.example.com",
            "http://localhost:3000",
            "https://*.example.com",
            "HTTPS://App.Example.com:8443",
        ] {
            assert!(validate_origin_pattern(ok).is_ok(), "{ok}");
        }
        for bad in [
            "app.example.com",
            "ftp://x.example.com",
            "https://app.example.com/",
            "https://app.example.com/path",
            "https://user@app.example.com",
            "https://*",
            "https://a.*.example.com",
            "https://*.",
        ] {
            assert!(validate_origin_pattern(bad).is_err(), "{bad}");
        }
    }
}
