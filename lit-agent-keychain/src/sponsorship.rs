use crate::{
    actions,
    api::{self, ApiError, ApiResult},
    auth::{SameOrigin, Session},
    billing,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::{field, Manifest, Signed},
    subscriptions,
};
use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use rand::RngCore;
use rocket::{http::Status, post, serde::json::Json, State};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;

fn unavailable(_: impl std::fmt::Display) -> ApiError {
    api::err(Status::BadGateway, "sponsorship_unavailable")
}
pub fn encrypt_key(key: &str, vault: &str, secret: &[u8; 32]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(secret).map_err(|_| anyhow::anyhow!("invalid key"))?;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: key.as_bytes(),
                aad: vault.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    Ok(hex::encode(
        [nonce.as_slice(), encrypted.as_slice()].concat(),
    ))
}
pub fn decrypt_key(value: &str, vault: &str, secret: &[u8; 32]) -> anyhow::Result<String> {
    let bytes = hex::decode(value)?;
    if bytes.len() < 28 || bytes.len() > 1024 {
        anyhow::bail!("invalid encrypted key");
    }
    let cipher = Aes256Gcm::new_from_slice(secret).map_err(|_| anyhow::anyhow!("invalid key"))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: vault.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    Ok(String::from_utf8(plaintext)?)
}
// Called with the vault lock held. An empty new group is safe even if a remote
// call succeeds and a later DB write fails: no key is handed out before commit.
async fn reconcile_locked(
    tx: &mut Transaction<'_, Postgres>,
    lit: &Chipotle,
    cfg: &Config,
    vault: &str,
    issue: bool,
    requested_cid: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let sub = subscriptions::lock(tx, vault).await?;
    let active = sub.plan(OffsetDateTime::now_utc()).active;
    let pending: Option<String> =
        sqlx::query_scalar("SELECT revoking_key FROM kc_execution_accounts WHERE vault_id=$1")
            .bind(vault)
            .fetch_optional(&mut **tx)
            .await
            .map_err(api::internal)?
            .flatten();
    if let Some(pending) = pending {
        let key =
            decrypt_key(&pending, vault, &cfg.usage_key_encryption_key).map_err(unavailable)?;
        lit.remove_usage_key(&key).await.map_err(unavailable)?;
        sqlx::query("UPDATE kc_execution_accounts SET revoking_key=NULL WHERE vault_id=$1")
            .bind(vault)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
        sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,'execution_key_revoked')")
            .bind(vault)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
    }
    let account: Option<(i64, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT group_id,secret_group_id,encrypted_key,scope_hash FROM kc_execution_accounts WHERE vault_id=$1",
    ).bind(vault).fetch_optional(&mut **tx).await.map_err(api::internal)?;
    let (group, secret_group, encrypted, old_hash) = if let Some(row) = account {
        row
    } else {
        if !issue {
            if requested_cid.is_some() {
                return Err(api::err(Status::Conflict, "execution_key_required"));
            }
            return Ok(None);
        }
        let authority: String =
            sqlx::query_scalar("SELECT authority_cid FROM kc_vaults WHERE id=$1")
                .bind(vault)
                .fetch_one(&mut **tx)
                .await
                .map_err(api::internal)?;
        let group = lit
            .create_group(vault, &[authority, actions::cid(actions::PUBLIC_KEY)])
            .await
            .map_err(unavailable)?;
        let secret_group = lit.create_group(vault, &[]).await.map_err(unavailable)?;
        sqlx::query(
            "INSERT INTO kc_execution_accounts(vault_id,group_id,secret_group_id) VALUES($1,$2,$3)",
        )
        .bind(vault)
        .bind(group)
        .bind(secret_group)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
        (group, secret_group, None, None)
    };
    let groups = if active {
        vec![group, secret_group]
    } else {
        vec![group]
    };
    let hash = crypto::digest(&json!(groups)).map_err(api::invalid)?;
    let existing_key = encrypted
        .as_deref()
        .map(|value| decrypt_key(value, vault, &cfg.usage_key_encryption_key))
        .transpose()
        .map_err(unavailable)?;
    // Toggle the secret group on the key itself. Never rebuild a thousand-CID
    // group on cancellation/renewal. Retrying an ambiguous response is idempotent.
    if old_hash.as_deref() != Some(&hash) {
        if let Some(key) = &existing_key {
            lit.update_usage_key(key, &groups)
                .await
                .map_err(unavailable)?;
            sqlx::query("UPDATE kc_execution_accounts SET scope_hash=$2 WHERE vault_id=$1")
                .bind(vault)
                .bind(&hash)
                .execute(&mut **tx)
                .await
                .map_err(api::internal)?;
        }
    }
    if active && !issue {
        // Bulk group replacement is capped at 10 CIDs by Chipotle's contract.
        // Incremental addition has no such cap and is idempotent. A bounded batch
        // resumes abandoned enrollments in the worker. Foreground enrollment
        // applies only its requested CID; issuing a key never waits on backlog.
        let pending: Vec<String> = sqlx::query_scalar("SELECT action_cid FROM kc_execution_actions WHERE vault_id=$1 AND NOT applied AND ($2::text IS NULL OR action_cid=$2) ORDER BY created_at,action_cid LIMIT 10")
            .bind(vault).bind(requested_cid).fetch_all(&mut **tx).await.map_err(api::internal)?;
        for cid in pending {
            lit.add_action(secret_group, &cid)
                .await
                .map_err(unavailable)?;
            sqlx::query(
                "UPDATE kc_execution_actions SET applied=true WHERE vault_id=$1 AND action_cid=$2",
            )
            .bind(vault)
            .bind(cid)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
        }
    }
    if !issue {
        return Ok(None);
    }
    if let Some(key) = existing_key {
        return Ok(Some(key));
    }
    let key = lit.create_usage_key(&groups).await.map_err(unavailable)?;
    let encrypted = encrypt_key(&key, vault, &cfg.usage_key_encryption_key).map_err(unavailable)?;
    sqlx::query(
        "UPDATE kc_execution_accounts SET encrypted_key=$2,scope_hash=$3 WHERE vault_id=$1",
    )
    .bind(vault)
    .bind(encrypted)
    .bind(hash)
    .execute(&mut **tx)
    .await
    .map_err(api::internal)?;
    sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,'execution_key_issued')")
        .bind(vault)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    Ok(Some(key))
}
pub async fn reconcile(
    pool: &PgPool,
    lit: &Chipotle,
    cfg: &Config,
    vault: &str,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(api::internal)?;
    reconcile_locked(&mut tx, lit, cfg, vault, false, None).await?;
    tx.commit().await.map_err(api::internal)
}
#[post("/api/execution-key")]
pub async fn key(
    _origin: SameOrigin,
    session: Session,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    billing::reserve(pool, &format!("usage-key:{}", session.vault_id), 3600, 30).await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let key = reconcile_locked(&mut tx, lit, cfg, &session.vault_id, true, None)
        .await?
        .ok_or_else(|| unavailable("missing key"))?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"usageApiKey":key})))
}
#[post("/api/execution-key/rotate")]
pub async fn rotate(
    _origin: SameOrigin,
    session: Session,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    billing::reserve(
        pool,
        &format!("usage-rotate:{}", session.vault_id),
        86400,
        10,
    )
    .await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    subscriptions::lock(&mut tx, &session.vault_id).await?;
    let encrypted: Option<String> =
        sqlx::query_scalar("SELECT encrypted_key FROM kc_execution_accounts WHERE vault_id=$1")
            .bind(&session.vault_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(api::internal)?
            .flatten();
    // Persist the pending revocation before calling Chipotle. Retrying rotation
    // reconciles it; no success is returned until the old key is removed.
    if let Some(encrypted) = encrypted {
        sqlx::query("UPDATE kc_execution_accounts SET encrypted_key=NULL,revoking_key=$2 WHERE vault_id=$1 AND revoking_key IS NULL")
            .bind(&session.vault_id)
            .bind(encrypted)
            .execute(&mut *tx)
            .await
            .map_err(api::internal)?;
        sqlx::query(
            "INSERT INTO kc_audit(vault_id,event) VALUES($1,'execution_key_rotation_requested')",
        )
        .bind(&session.vault_id)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    }
    tx.commit().await.map_err(api::internal)?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let key = reconcile_locked(&mut tx, lit, cfg, &session.vault_id, true, None)
        .await?
        .ok_or_else(|| unavailable("missing key"))?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"usageApiKey":key})))
}
#[post("/api/actions", format = "json", data = "<body>")]
pub async fn enroll(
    _origin: SameOrigin,
    session: Session,
    body: Json<Signed>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    let vault = &session.vault_id;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let plan = subscriptions::require_active(&mut tx, vault).await?;
    let authority: String = sqlx::query_scalar("SELECT authority_cid FROM kc_vaults WHERE id=$1")
        .bind(vault)
        .fetch_one(&mut *tx)
        .await
        .map_err(api::internal)?;
    let key = lit.public_key(&authority).await.map_err(unavailable)?;
    crypto::verify_signed(&body, &key, vault).map_err(api::denied)?;
    if field(&body.document, "kind").map_err(api::invalid)? != "manifest" {
        return Err(api::err(Status::BadRequest, "manifest_required"));
    }
    let manifest: Manifest =
        serde_json::from_value(body.document["manifest"].clone()).map_err(api::invalid)?;
    manifest.validate(cfg).map_err(api::invalid)?;
    let cid = actions::cid(&actions::secret_source(&manifest).map_err(api::invalid)?);
    if manifest.vault_id != *vault
        || manifest.authority_cid != authority
        || field(&body.document, "actionCid").map_err(api::invalid)? != cid
    {
        return Err(api::err(Status::Forbidden, "wrong_manifest"));
    }
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT action_cid FROM kc_execution_actions WHERE vault_id=$1 AND secret_id=$2",
    )
    .bind(vault)
    .bind(&manifest.secret_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(api::internal)?;
    if let Some(existing) = existing {
        if existing != cid {
            return Err(api::err(Status::Conflict, "action_already_registered"));
        }
    } else {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM kc_execution_actions WHERE vault_id=$1")
                .bind(vault)
                .fetch_one(&mut *tx)
                .await
                .map_err(api::internal)?;
        if count >= plan.secret_limit + 100 {
            return Err(api::err(Status::Conflict, "secret_limit_reached"));
        }
        sqlx::query(
            "INSERT INTO kc_execution_actions(vault_id,secret_id,action_cid) VALUES($1,$2,$3)",
        )
        .bind(vault)
        .bind(&manifest.secret_id)
        .bind(&cid)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
        sqlx::query(
            "INSERT INTO kc_audit(vault_id,event,object_hash) VALUES($1,'action_enrolled',$2)",
        )
        .bind(vault)
        .bind(&cid)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    }
    // Commit desired scope first. A crash after the Chipotle write can then be
    // retried from durable state without leaving an untracked granted CID.
    tx.commit().await.map_err(api::internal)?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    reconcile_locked(&mut tx, lit, cfg, vault, false, Some(&cid)).await?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"ok":true})))
}
