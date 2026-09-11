//! Agent-facing data-plane endpoints.
//!
//! * `POST /api/grants` — policy check, then a signed grant the agent hands to
//!   the reader action on Chipotle. The plaintext flows Chipotle → agent; this
//!   service never sees it.
//! * `GET /api/reference/<name>` — ciphertext + vault id for the in-TEE-only
//!   tier, so the customer's own permitted action can `Decrypt` it.

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, post, State};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::actions::ActionSet;
use crate::agents::AgentKey;
use crate::api::{err, err_detail, internal, ApiError, ApiResult};
use crate::audit::{self, Event, SecretRef};
use crate::chipotle::ChipotleClient;
use crate::config::Config;
use crate::policy::{self, Denial, GrantContext};
use crate::secrets::{self, SecretRow, VersionRow};
use crate::signer::GrantSigner;
use crate::tenants::{self, Tenant};

/// Canonical grant document. Field order matters: the signed string is the
/// serde_json serialization of this struct, verified verbatim in the reader.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub v: u8,
    pub tenant: Uuid,
    pub name: String,
    pub version: i32,
    pub pkp_id: String,
    pub ciphertext_hash: String,
    pub release: &'static str,
    pub agent: Uuid,
    pub iat: i64,
    pub exp: i64,
}

impl Grant {
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[derive(Debug, Deserialize)]
pub struct GrantRequest {
    pub name: String,
    pub version: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ReaderAction {
    pub cid: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct GrantResponse {
    pub name: String,
    pub version: i32,
    pub grant: String,
    pub signature: String,
    pub ciphertext: String,
    pub pkp_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub chipotle_api_base_url: String,
    pub action: ReaderAction,
    /// Ready-to-send `js_params` for `POST {chipotle}/core/v1/lit_action`.
    pub js_params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ReferenceResponse {
    pub name: String,
    pub version: i32,
    pub release: policy::Release,
    pub ciphertext: String,
    pub ciphertext_hash: String,
    pub pkp_id: String,
    pub group_id: i64,
    pub chipotle_api_base_url: String,
}

async fn load_for_agent(
    pool: &PgPool,
    agent: &AgentKey,
    name: &str,
    version: Option<i32>,
    event: Event,
) -> Result<(Tenant, SecretRow, VersionRow), ApiError> {
    let tenant = tenants::find_by_id(pool, agent.tenant_id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
        .ok_or_else(|| err(Status::NotFound, "not_found"))?;
    let Some(secret) = secrets::find_secret(pool, tenant.id, name)
        .await
        .map_err(|e| internal("secret_lookup_failed", e))?
    else {
        audit::record(
            pool,
            tenant.id,
            SecretRef { id: None, name },
            agent,
            event,
            false,
            Some("secret_not_found"),
        )
        .await;
        return Err(err(Status::NotFound, "not_found"));
    };
    let wanted = version.unwrap_or(secret.current_version);
    let Some(ver) = secrets::find_version(pool, secret.id, wanted)
        .await
        .map_err(|e| internal("version_lookup_failed", e))?
    else {
        audit::record(
            pool,
            tenant.id,
            SecretRef {
                id: Some(secret.id),
                name: &secret.name,
            },
            agent,
            event,
            false,
            Some("version_not_found"),
        )
        .await;
        return Err(err(Status::NotFound, "version_not_found"));
    };
    Ok((tenant, secret, ver))
}

fn deny(d: Denial) -> ApiError {
    err_detail(Status::Forbidden, d.code(), "policy denied this request")
}

#[post("/api/grants", data = "<req>")]
pub async fn issue_grant(
    agent: AgentKey,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    signer: &State<GrantSigner>,
    actions: &State<ActionSet>,
    chipotle: &State<ChipotleClient>,
    req: Json<GrantRequest>,
) -> ApiResult<GrantResponse> {
    let req = req.into_inner();
    let name = req.name.trim();
    let (tenant, secret, ver) =
        load_for_agent(pool, &agent, name, req.version, Event::Grant).await?;

    if tenant.reader_cid != actions.reader_cid {
        // Signer rotated since this tenant was provisioned; the reader this
        // deployment would hand out isn't in the tenant's group yet.
        tracing::error!(tenant_id = %tenant.id, "reader CID stale for tenant; refusing grant");
        return Err(err(Status::ServiceUnavailable, "reader_not_attached"));
    }

    // Quota check + allow-record are one transaction under a per-secret
    // advisory lock, so concurrent requests can't all read the same count and
    // collectively blow past max_reads_per_day, and a grant is never issued
    // without its audit row (codex findings, 2026-09-10).
    let now = OffsetDateTime::now_utc();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| internal("tx_begin_failed", e))?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(audit::secret_lock_key(secret.id))
        .execute(&mut *tx)
        .await
        .map_err(|e| internal("quota_lock_failed", e))?;
    // Re-read the secret under a share lock now that we hold the advisory
    // lock: the row loaded above may have been deleted or its policy changed
    // while we waited. Without this, access_log no longer having a FK to
    // secrets would let a grant for a deleted secret slip through (codex
    // finding, 2026-09-10). The share lock also blocks a concurrent DELETE
    // until this transaction commits.
    let Some(secret) = secrets::lock_secret_for_share(&mut *tx, secret.id)
        .await
        .map_err(|e| internal("secret_relock_failed", e))?
    else {
        drop(tx);
        audit::record(
            pool,
            tenant.id,
            SecretRef {
                id: Some(secret.id),
                name: &secret.name,
            },
            &agent,
            Event::Grant,
            false,
            Some("secret_not_found"),
        )
        .await;
        return Err(err(Status::NotFound, "not_found"));
    };
    let reads = audit::grants_last_24h(&mut *tx, secret.id)
        .await
        .map_err(|e| internal("audit_count_failed", e))?;
    let ctx = GrantContext {
        disabled: secret.disabled,
        release: secret.release,
        agent_id: agent.id,
        reads_last_24h: reads,
        now,
    };
    if let Err(d) = policy::evaluate_grant(&secret.policy, &ctx) {
        drop(tx);
        audit::record(
            pool,
            tenant.id,
            SecretRef {
                id: Some(secret.id),
                name: &secret.name,
            },
            &agent,
            Event::Grant,
            false,
            Some(d.code()),
        )
        .await;
        return Err(deny(d));
    }
    audit::record_allow_tx(&mut tx, tenant.id, &secret, &agent, Event::Grant)
        .await
        .map_err(|e| internal("audit_insert_failed", e))?;
    tx.commit()
        .await
        .map_err(|e| internal("tx_commit_failed", e))?;

    let exp = now + time::Duration::seconds(cfg.grant_ttl_secs);
    let grant = Grant {
        v: 1,
        tenant: tenant.id,
        name: secret.name.clone(),
        version: ver.version,
        pkp_id: tenant.pkp_id.clone(),
        ciphertext_hash: ver.ciphertext_hash.clone(),
        release: "plaintext",
        agent: agent.id,
        iat: now.unix_timestamp(),
        exp: exp.unix_timestamp(),
    };
    let grant_json = grant
        .canonical_json()
        .map_err(|e| internal("grant_encode_failed", e))?;
    let signature = signer
        .sign_message(&grant_json)
        .map_err(|e| internal("grant_sign_failed", e))?;

    let js_params = serde_json::json!({
        "grant": grant_json,
        "signature": signature,
        "ciphertext": ver.ciphertext,
        "pkpId": tenant.pkp_id,
    });
    Ok(Json(GrantResponse {
        name: secret.name,
        version: ver.version,
        grant: grant_json,
        signature,
        ciphertext: ver.ciphertext,
        pkp_id: tenant.pkp_id,
        expires_at: exp,
        chipotle_api_base_url: chipotle.base_url().to_string(),
        action: ReaderAction {
            cid: actions.reader_cid.clone(),
            code: actions.reader_code.clone(),
        },
        js_params,
    }))
}

#[get("/api/reference/<name>?<version>")]
pub async fn get_reference(
    agent: AgentKey,
    pool: &State<PgPool>,
    chipotle: &State<ChipotleClient>,
    name: &str,
    version: Option<i32>,
) -> ApiResult<ReferenceResponse> {
    let (tenant, secret, ver) =
        load_for_agent(pool, &agent, name.trim(), version, Event::Reference).await?;
    if let Err(d) = policy::evaluate_reference(
        &secret.policy,
        secret.disabled,
        agent.id,
        OffsetDateTime::now_utc(),
    ) {
        audit::record(
            pool,
            tenant.id,
            SecretRef {
                id: Some(secret.id),
                name: &secret.name,
            },
            &agent,
            Event::Reference,
            false,
            Some(d.code()),
        )
        .await;
        return Err(deny(d));
    }
    audit::record(
        pool,
        tenant.id,
        SecretRef {
            id: Some(secret.id),
            name: &secret.name,
        },
        &agent,
        Event::Reference,
        true,
        None,
    )
    .await;
    Ok(Json(ReferenceResponse {
        name: secret.name,
        version: ver.version,
        release: secret.release,
        ciphertext: ver.ciphertext,
        ciphertext_hash: ver.ciphertext_hash,
        pkp_id: tenant.pkp_id,
        group_id: tenant.group_id,
        chipotle_api_base_url: chipotle.base_url().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_json_is_stable_and_camel_case() {
        let g = Grant {
            v: 1,
            tenant: Uuid::nil(),
            name: "K".into(),
            version: 2,
            pkp_id: "0xabc".into(),
            ciphertext_hash: "0xdef".into(),
            release: "plaintext",
            agent: Uuid::nil(),
            iat: 1,
            exp: 2,
        };
        let s = g.canonical_json().unwrap();
        assert!(s.starts_with("{\"v\":1,\"tenant\":"));
        assert!(s.contains("\"pkpId\":\"0xabc\""));
        assert!(s.contains("\"ciphertextHash\":\"0xdef\""));
        assert!(s.ends_with("\"iat\":1,\"exp\":2}"));
        let back: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back, serde_json::to_value(&g).unwrap());
        assert_eq!(back["agent"], Uuid::nil().to_string());
    }
}
