//! Key-possession discovery is metadata/billing authentication, never Lit authority.
use crate::{
    api::{self, ApiResult},
    auth::SameOrigin,
    billing,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::{field, number, valid_hex, Manifest},
    registry, sponsorship,
};
use anyhow::{bail, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use rocket::{http::Status, post, serde::json::Json, State};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;

pub const DOMAIN: &str = "lit-keychain/discovery/v2";
pub fn validate_challenge(c: &Value, audience: &str, now: i64) -> Result<()> {
    if c.as_object().is_none_or(|o| o.len() != 7)
        || c["v"] != 2
        || field(c, "domain")? != DOMAIN
        || field(c, "audience")? != audience
        || !valid_hex(field(c, "agentPublicKey")?, 32)
        || !valid_hex(field(c, "nonce")?, 32)
    {
        bail!("invalid challenge");
    }
    let start = number(c, "issuedAt")?;
    let end = number(c, "expiresAt")?;
    if start > now + 30 || end <= now || end <= start || end - start > 60 {
        bail!("invalid challenge window");
    }
    Ok(())
}
pub fn verify_proof(c: &Value, signature: &str, audience: &str, now: i64) -> Result<()> {
    validate_challenge(c, audience, now)?;
    if !valid_hex(signature, 64) {
        bail!("invalid signature");
    }
    let bytes: [u8; 32] = hex::decode(field(c, "agentPublicKey")?)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid key"))?;
    let key = VerifyingKey::from_bytes(&bytes)?;
    key.verify_strict(
        &hex::decode(crypto::digest(c)?)?,
        &Signature::from_slice(&hex::decode(signature)?)?,
    )?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    agent_public_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    challenge: Value,
    signature: String,
}

#[post("/api/agents/challenge", format = "json", data = "<body>")]
pub async fn challenge(
    _origin: SameOrigin,
    peer: billing::Peer,
    body: Json<Identity>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
) -> ApiResult<Value> {
    if !valid_hex(&body.agent_public_key, 32) {
        return Err(api::invalid("key"));
    }
    // Global first bounds the number of attacker-selected peer/key budget rows.
    billing::reserve(pool, "agent-challenge-global", 60, 10000).await?;
    billing::reserve(pool, &format!("agent-challenge-peer:{}", peer.0), 60, 600).await?;
    sqlx::query("DELETE FROM kc_agent_challenges WHERE expires_at<=now()")
        .execute(pool.inner())
        .await
        .map_err(api::internal)?;
    let now = time::OffsetDateTime::now_utc();
    let expires = now + time::Duration::seconds(60);
    let c = json!({"v":2,"domain":DOMAIN,"audience":cfg.public_base_url,"agentPublicKey":body.agent_public_key,"nonce":crypto::random_token(),"issuedAt":now.unix_timestamp(),"expiresAt":expires.unix_timestamp()});
    sqlx::query("INSERT INTO kc_agent_challenges(nonce,challenge,expires_at) VALUES($1,$2,$3)")
        .bind(field(&c, "nonce").map_err(api::invalid)?)
        .bind(&c)
        .bind(expires)
        .execute(pool.inner())
        .await
        .map_err(api::internal)?;
    Ok(Json(c))
}

// Deliberately matches the action's current policy/operation/envelope checks.
// Signature and all document bindings are checked separately before disclosure.
pub fn active_grant(
    policy: &Value,
    agent: &str,
    operation: &str,
    version: i64,
    envelope_hash: &str,
    now: i64,
) -> bool {
    policy["v"] == 2
        && policy["domain"] == "lit-keychain/v2"
        && policy["disabled"] == false
        && number(policy, "notBefore").is_ok_and(|n| n <= now)
        && (policy.get("expiresAt") == Some(&Value::Null)
            || number(policy, "expiresAt").is_ok_and(|n| n > now))
        && policy["grants"].as_array().is_some_and(|grants| {
            grants
                .iter()
                .find(|g| g["agentPublicKey"] == agent)
                .is_some_and(|g| {
                    g["operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op == operation))
                        && g["versions"].as_array().is_some_and(|vs| {
                            vs.iter().any(|v| {
                                v["version"] == version && v["envelopeHash"] == envelope_hash
                            })
                        })
                })
        })
}

#[post("/api/agents/discover", format = "json", data = "<body>")]
pub async fn discover(
    _origin: SameOrigin,
    peer: billing::Peer,
    body: Json<Proof>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
) -> ApiResult<Value> {
    billing::reserve(pool, "agent-discovery-global", 60, 10000).await?;
    billing::reserve(pool, &format!("agent-discovery-peer:{}", peer.0), 60, 600).await?;
    verify_proof(
        &body.challenge,
        &body.signature,
        &cfg.public_base_url,
        time::OffsetDateTime::now_utc().unix_timestamp(),
    )
    .map_err(api::denied)?;
    // Exact stored challenge; atomic consumption also works across service replicas.
    let consumed = sqlx::query(
        "DELETE FROM kc_agent_challenges WHERE nonce=$1 AND challenge=$2 AND expires_at>now()",
    )
    .bind(field(&body.challenge, "nonce").map_err(api::denied)?)
    .bind(&body.challenge)
    .execute(pool.inner())
    .await
    .map_err(api::internal)?;
    if consumed.rows_affected() != 1 {
        return Err(api::denied("used or expired"));
    }
    let agent = field(&body.challenge, "agentPublicKey").map_err(api::denied)?;
    let rows: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('manifest',s.manifest,'envelope',e.signed,'policy',p.signed) FROM kc_secrets s JOIN kc_envelopes e ON e.secret_id=s.id AND e.version=s.current_version JOIN kc_registry r ON r.scope='secret:'||s.id JOIN kc_policies p ON p.hash=r.policy_hash WHERE NOT s.archived AND p.signed->'document'->'grants' @> $1 ORDER BY s.id LIMIT 1001")
        .bind(json!([{"agentPublicKey":agent}])).fetch_all(pool.inner()).await.map_err(api::internal)?;
    if rows.len() > 1000 {
        return Err(api::err(Status::Conflict, "discovery_limit_exceeded"));
    }
    let mut keys = HashMap::<String, String>::new();
    let mut secrets = Vec::new();
    for row in rows {
        let bundle: registry::SecretWrite = serde_json::from_value(row).map_err(api::internal)?;
        let manifest: Manifest =
            serde_json::from_value(bundle.manifest.document["manifest"].clone())
                .map_err(api::internal)?;
        let Some(release) = crate::actions::release(&manifest.release) else {
            return Err(api::denied("release"));
        };
        let envelope = &bundle.envelope.document;
        if !active_grant(
            &bundle.policy.document,
            agent,
            &release.operation,
            number(&envelope["metadata"], "version").map_err(api::denied)?,
            &crypto::digest(envelope).map_err(api::denied)?,
            time::OffsetDateTime::now_utc().unix_timestamp(),
        ) {
            continue;
        }
        let authority_key =
            crate::authority::key_for(pool, lit, &manifest.vault_id, &manifest.authority_cid)
                .await?;
        registry::validate_write(
            &bundle,
            &manifest.vault_id,
            &authority_key,
            cfg,
            false,
            false,
        )
        .map_err(api::denied)?;
        // Only an already-provisioned execution-only key belonging to this exact
        // approved vault. Never mint an account or return bootstrap/master keys.
        let usage_key = if let Some(key) = keys.get(&manifest.vault_id) {
            key.clone()
        } else {
            let encrypted: Option<String> = sqlx::query_scalar("SELECT encrypted_key FROM kc_execution_accounts WHERE vault_id=$1 AND revoking_key IS NULL")
                .bind(&manifest.vault_id).fetch_optional(pool.inner()).await.map_err(api::internal)?.flatten();
            let encrypted = encrypted.ok_or_else(|| {
                api::err(
                    Status::ServiceUnavailable,
                    "execution_bootstrap_unavailable",
                )
            })?;
            let key = sponsorship::decrypt_key(
                &encrypted,
                &manifest.vault_id,
                &cfg.usage_key_encryption_key,
            )
            .map_err(api::internal)?;
            keys.insert(manifest.vault_id.clone(), key.clone());
            key
        };
        secrets.push(json!({"name":envelope["metadata"]["name"],"manifest":manifest,"actionCid":bundle.manifest.document["actionCid"],"usageApiKey":usage_key}));
    }
    Ok(Json(json!({"v":2,"secrets":secrets})))
}
