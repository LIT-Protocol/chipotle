//! Per-secret access policy, evaluated by the control plane before a grant is
//! signed. The reader action enforces the *grant*; this module decides whether
//! a grant should exist at all.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Release {
    /// Authorized agents may read the value (via the reader action). Turnkey parity.
    Plaintext,
    /// Only actions in the tenant group may decrypt; no grants are ever issued.
    InTeeOnly,
}

impl Release {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::InTeeOnly => "in_tee_only",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "plaintext" => Some(Self::Plaintext),
            "in_tee_only" => Some(Self::InTeeOnly),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Agent ids allowed to read. `None` = every agent in the tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_agents: Option<Vec<Uuid>>,
    /// Rolling 24h cap on successful grants for this secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_reads_per_day: Option<i64>,
    /// Hard expiry after which no grants are issued.
    #[serde(
        default,
        with = "time::serde::rfc3339::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub not_after: Option<OffsetDateTime>,
}

impl Policy {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(n) = self.max_reads_per_day {
            if n <= 0 {
                return Err("max_reads_per_day must be positive");
            }
        }
        if let Some(agents) = &self.allowed_agents {
            if agents.is_empty() {
                return Err("allowed_agents must be omitted or non-empty");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    SecretDisabled,
    ReleaseNotPlaintext,
    AgentNotAllowed,
    PolicyExpired,
    RateLimited,
}

impl Denial {
    pub fn code(self) -> &'static str {
        match self {
            Self::SecretDisabled => "secret_disabled",
            Self::ReleaseNotPlaintext => "release_not_plaintext",
            Self::AgentNotAllowed => "agent_not_allowed",
            Self::PolicyExpired => "policy_expired",
            Self::RateLimited => "rate_limited",
        }
    }
}

pub struct GrantContext {
    pub disabled: bool,
    pub release: Release,
    pub agent_id: Uuid,
    pub reads_last_24h: i64,
    pub now: OffsetDateTime,
}

/// Decide whether a plaintext grant may be issued.
pub fn evaluate_grant(policy: &Policy, ctx: &GrantContext) -> Result<(), Denial> {
    if ctx.disabled {
        return Err(Denial::SecretDisabled);
    }
    if ctx.release != Release::Plaintext {
        return Err(Denial::ReleaseNotPlaintext);
    }
    if let Some(not_after) = policy.not_after {
        if ctx.now >= not_after {
            return Err(Denial::PolicyExpired);
        }
    }
    if let Some(allowed) = &policy.allowed_agents {
        if !allowed.contains(&ctx.agent_id) {
            return Err(Denial::AgentNotAllowed);
        }
    }
    if let Some(max) = policy.max_reads_per_day {
        if ctx.reads_last_24h >= max {
            return Err(Denial::RateLimited);
        }
    }
    Ok(())
}

/// Decide whether an agent may fetch the ciphertext reference (any tier).
/// Ciphertext is public-safe, so only the agent allowlist and disabled flag apply.
pub fn evaluate_reference(policy: &Policy, disabled: bool, agent_id: Uuid) -> Result<(), Denial> {
    if disabled {
        return Err(Denial::SecretDisabled);
    }
    if let Some(allowed) = &policy.allowed_agents {
        if !allowed.contains(&agent_id) {
            return Err(Denial::AgentNotAllowed);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(agent: Uuid) -> GrantContext {
        GrantContext {
            disabled: false,
            release: Release::Plaintext,
            agent_id: agent,
            reads_last_24h: 0,
            now: OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap(),
        }
    }

    #[test]
    fn default_policy_allows_plaintext() {
        let a = Uuid::new_v4();
        assert_eq!(evaluate_grant(&Policy::default(), &ctx(a)), Ok(()));
    }

    #[test]
    fn in_tee_only_never_grants() {
        let a = Uuid::new_v4();
        let mut c = ctx(a);
        c.release = Release::InTeeOnly;
        assert_eq!(
            evaluate_grant(&Policy::default(), &c),
            Err(Denial::ReleaseNotPlaintext)
        );
        // …but references are fine.
        assert_eq!(evaluate_reference(&Policy::default(), false, a), Ok(()));
    }

    #[test]
    fn agent_allowlist() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let p = Policy {
            allowed_agents: Some(vec![a]),
            ..Default::default()
        };
        assert_eq!(evaluate_grant(&p, &ctx(a)), Ok(()));
        assert_eq!(evaluate_grant(&p, &ctx(b)), Err(Denial::AgentNotAllowed));
        assert_eq!(
            evaluate_reference(&p, false, b),
            Err(Denial::AgentNotAllowed)
        );
    }

    #[test]
    fn rate_limit_and_expiry() {
        let a = Uuid::new_v4();
        let p = Policy {
            max_reads_per_day: Some(2),
            not_after: Some(OffsetDateTime::from_unix_timestamp(1_800_000_001).unwrap()),
            ..Default::default()
        };
        let mut c = ctx(a);
        assert_eq!(evaluate_grant(&p, &c), Ok(()));
        c.reads_last_24h = 2;
        assert_eq!(evaluate_grant(&p, &c), Err(Denial::RateLimited));
        c.reads_last_24h = 0;
        c.now = OffsetDateTime::from_unix_timestamp(1_800_000_001).unwrap();
        assert_eq!(evaluate_grant(&p, &c), Err(Denial::PolicyExpired));
    }

    #[test]
    fn disabled_wins() {
        let a = Uuid::new_v4();
        let mut c = ctx(a);
        c.disabled = true;
        assert_eq!(
            evaluate_grant(&Policy::default(), &c),
            Err(Denial::SecretDisabled)
        );
        assert_eq!(
            evaluate_reference(&Policy::default(), true, a),
            Err(Denial::SecretDisabled)
        );
    }

    #[test]
    fn validation_and_roundtrip() {
        assert!(Policy {
            max_reads_per_day: Some(0),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Policy {
            allowed_agents: Some(vec![]),
            ..Default::default()
        }
        .validate()
        .is_err());
        let p: Policy = serde_json::from_str(r#"{"max_reads_per_day": 5}"#).unwrap();
        assert_eq!(p.max_reads_per_day, Some(5));
        assert!(serde_json::from_str::<Policy>(r#"{"bogus": 1}"#).is_err());
        assert_eq!(
            serde_json::to_value(Policy::default()).unwrap(),
            serde_json::json!({})
        );
    }
}
