//! Honest-registry replay protection, also used by independently funded Lit calls.
use crate::{
    api::{self, ApiResult},
    crypto,
    models::{field, number, valid_hex},
    registry,
};
use anyhow::{bail, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use rocket::{http::Status, post, serde::json::Json, State};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub request: Value,
    pub signature: String,
}

pub fn verify(proof: &Proof, now: i64) -> Result<()> {
    let r = &proof.request;
    if r["v"] != 2 || field(r, "domain")? != "lit-keychain/v2" {
        bail!("invalid request");
    }
    for name in [
        "vaultId",
        "secretId",
        "agentPublicKey",
        "nonce",
        "policyHash",
        "envelopeHash",
    ] {
        if !valid_hex(field(r, name)?, 32) {
            bail!("invalid request binding");
        }
    }
    let start = number(r, "issuedAt")?;
    let end = number(r, "expiresAt")?;
    if start < 0 || start > now + 30 || end <= now || end <= start || end - start > 120 {
        bail!("invalid request window");
    }
    let bytes: [u8; 32] = hex::decode(field(r, "agentPublicKey")?)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid key"))?;
    VerifyingKey::from_bytes(&bytes)?.verify_strict(
        &hex::decode(crypto::digest(r)?)?,
        &Signature::from_slice(&hex::decode(&proof.signature)?)?,
    )?;
    Ok(())
}

/// The unique insert is the arbitration point. Never return success on conflict,
/// even for an identical request or after a lost response. Keep the claim on failure.
pub async fn claim(pool: &PgPool, proof: &Proof) -> Result<bool> {
    let r = &proof.request;
    verify(proof, time::OffsetDateTime::now_utc().unix_timestamp())?;
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO kc_used_requests(vault_id,agent_public_key,operation,nonce,expires_at) \
         SELECT $1,$2,$3,$4,to_timestamp($5) WHERE to_timestamp($5)>clock_timestamp() \
         ON CONFLICT DO NOTHING",
    )
    .bind(field(r, "vaultId")?)
    .bind(field(r, "agentPublicKey")?)
    .bind(field(r, "operation")?)
    .bind(field(r, "nonce")?)
    .bind(number(r, "expiresAt")? as f64)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if inserted {
        sqlx::query("INSERT INTO kc_audit(vault_id,event,object_hash) VALUES($1,'agent_request_consumed',$2)")
            .bind(field(r, "vaultId")?).bind(crypto::digest(r)?)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(inserted)
}

#[post("/api/registry/requests/consume", format = "json", data = "<body>")]
pub async fn consume(body: Json<Proof>, pool: &State<PgPool>) -> ApiResult<Value> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    verify(&body, now).map_err(api::denied)?;
    let r = &body.request;
    let secret = field(r, "secretId").map_err(api::denied)?;
    let selected = registry::selected(pool, &format!("secret:{secret}"))
        .await?
        .ok_or_else(|| api::err(Status::Forbidden, "access_denied"))?;
    let p = &selected["document"];
    let permitted = p["vaultId"] == r["vaultId"]
        && p["secretId"] == r["secretId"]
        && p["actionCid"] == r["actionCid"]
        && p["disabled"] == false
        && crypto::digest(p).map_err(api::denied)?
            == field(r, "policyHash").map_err(api::denied)?
        && number(p, "notBefore").map_err(api::denied)? <= now
        && (p["expiresAt"].is_null() || number(p, "expiresAt").map_err(api::denied)? > now)
        && p["grants"].as_array().is_some_and(|grants| {
            grants.iter().any(|g| {
                g["agentPublicKey"] == r["agentPublicKey"]
                    && g["operations"]
                        .as_array()
                        .is_some_and(|ops| ops.contains(&r["operation"]))
                    && g["versions"].as_array().is_some_and(|versions| {
                        versions.iter().any(|v| {
                            v["version"] == r["version"] && v["envelopeHash"] == r["envelopeHash"]
                        })
                    })
            })
        });
    if !permitted {
        return Err(api::err(Status::Forbidden, "access_denied"));
    }
    if !claim(pool, &body).await.map_err(api::internal)? {
        return Err(api::err(Status::Conflict, "request_already_consumed"));
    }
    Ok(Json(json!({"ok":true})))
}
